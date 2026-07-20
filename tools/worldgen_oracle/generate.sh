#!/usr/bin/env bash
set -euo pipefail

# Generate lossless, Java-authored final-chunk fixtures with the official 26.2
# dedicated server. The server JAR is intentionally external to the repository.
# It is verified before use so a fixture cannot silently switch game versions.

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
server_jar=${MINECRAFT_26_2_SERVER_JAR:-/tmp/minecraft-26.2-server.jar}
fixture_dir=${1:-"$root_dir/tests/fixtures/worldgen/minecraft-26.2"}
seed=${WORLDGEN_ORACLE_SEED:-42}
expected_sha1=823e2250d24b3ddac457a60c92a6a941943fcd6a
# Stay outside spawn preparation. Spawn chunks can receive random ticks before
# console input is available, which would turn a generation fixture into a
# post-generation simulation fixture.
if [[ -n ${WORLDGEN_ORACLE_CHUNKS:-} ]]; then
    IFS=';' read -r -a chunks <<<"$WORLDGEN_ORACLE_CHUNKS"
else
    chunks=("10 10" "-10 -10" "11 10" "10 11")
fi

if [[ ! -f "$server_jar" ]]; then
    echo "missing 26.2 server JAR: $server_jar" >&2
    exit 1
fi
if [[ $(sha1sum "$server_jar" | awk '{print $1}') != "$expected_sha1" ]]; then
    echo "server JAR hash does not match Minecraft 26.2" >&2
    exit 1
fi

runtime_dir=$(mktemp -d "${TMPDIR:-/tmp}/vibecraft-worldgen-oracle.XXXXXX")
cleanup() {
    if [[ ${WORLDGEN_ORACLE_KEEP_RUNTIME:-0} == 1 ]]; then
        echo "preserved oracle runtime: $runtime_dir" >&2
    else
        rm -rf "$runtime_dir"
    fi
}
trap cleanup EXIT

mkdir -p "$fixture_dir/chunks"
cat >"$runtime_dir/eula.txt" <<'EOF'
eula=true
EOF
cat >"$runtime_dir/server.properties" <<EOF
level-name=oracle
level-seed=$seed
online-mode=false
spawn-protection=0
pause-when-empty-seconds=-1
view-distance=2
simulation-distance=2
motd=Vibecraft worldgen oracle
EOF

# First invocation unpacks the official bundled server/runtime in the isolated
# directory. The server then writes a normal Java world which becomes the oracle.
(
    cd "$runtime_dir"
    coproc SERVER { exec java -Xms512M -Xmx1G -jar "$server_jar" --nogui >server.log 2>&1; }
    server_pid=$SERVER_PID
    for _ in $(seq 1 120); do
        if grep -q "Done (" server.log; then break; fi
        if ! kill -0 "$server_pid" 2>/dev/null; then cat server.log >&2; exit 1; fi
        sleep 1
    done
    grep -q "Done (" server.log || { cat server.log >&2; exit 1; }
    printf 'seed\n' >&"${SERVER[1]}"
    sleep 1
    grep -Eq "Seed:.*\\[$seed\\]" server.log || { echo "server did not confirm seed $seed" >&2; cat server.log >&2; exit 1; }
    # Freeze before loading oracle chunks, then advance a fixed number of
    # ticks. This removes wall-clock scheduling from the capture protocol.
    printf 'tick freeze\ngamerule randomTickSpeed 0\n' >&"${SERVER[1]}"
    for pair in "${chunks[@]}"; do
        read -r x z <<<"$pair"
        block_x=$((x * 16))
        block_z=$((z * 16))
        printf 'forceload add %s %s %s %s\n' "$block_x" "$block_z" "$block_x" "$block_z" >&"${SERVER[1]}"
    done
    printf 'tick step 200\n' >&"${SERVER[1]}"
    sleep 12
    printf 'save-all flush\nstop\n' >&"${SERVER[1]}"
    wait "$server_pid"
)

# Compile against the exact server and bundled libraries that produced the world.
classpath="$runtime_dir/versions/26.2/server-26.2.jar"
while IFS= read -r -d '' library; do classpath+=":$library"; done < <(find "$runtime_dir/libraries" -name '*.jar' -print0)
mkdir -p "$runtime_dir/classes"
javac -cp "$classpath" -d "$runtime_dir/classes" "$root_dir/tools/worldgen_oracle/WorldgenFixtureExporter.java"

metadata_tmp=$(mktemp "$runtime_dir/metadata.XXXXXX")
printf '{\n  "schema": 1,\n  "minecraft_version": "26.2",\n  "server_sha1": "%s",\n  "seed": %s,\n  "chunks": [\n' "$expected_sha1" "$seed" >"$metadata_tmp"
for index in "${!chunks[@]}"; do
    read -r x z <<<"${chunks[$index]}"
    chunk_file="$fixture_dir/chunks/chunk.${x}.${z}.nbt"
    result=$(cd "$runtime_dir" && java -cp "$runtime_dir/classes:$classpath" WorldgenFixtureExporter "$runtime_dir/oracle/dimensions/minecraft/overworld/region" "$x" "$z" "$chunk_file")
    printf '    %s' "$result" >>"$metadata_tmp"
    if (( index + 1 < ${#chunks[@]} )); then printf ',' >>"$metadata_tmp"; fi
    printf '\n' >>"$metadata_tmp"
done
printf '  ]\n}\n' >>"$metadata_tmp"
mv "$metadata_tmp" "$fixture_dir/manifest.json"

echo "generated $fixture_dir from official Minecraft 26.2 server seed $seed"
