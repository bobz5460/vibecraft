#!/usr/bin/env bash
set -euo pipefail

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
server_jar=${MINECRAFT_26_2_SERVER_JAR:-/tmp/minecraft-26.2-server.jar}
seed=${1:-42}
runtime_dir=$(mktemp -d "${TMPDIR:-/tmp}/vibecraft-worldgen-in-process.XXXXXX")
trap 'rm -rf "$runtime_dir"' EXIT

[[ -f "$server_jar" ]] || { echo "missing server JAR: $server_jar" >&2; exit 1; }
[[ $(sha1sum "$server_jar" | awk '{print $1}') == 823e2250d24b3ddac457a60c92a6a941943fcd6a ]] || {
  echo "server JAR is not Minecraft 26.2" >&2; exit 1;
}

# The official bundler supplies the exact class path; --help only unpacks it.
(cd "$runtime_dir" && java -jar "$server_jar" --help >/dev/null 2>&1)
classpath="$runtime_dir/versions/26.2/server-26.2.jar"
while IFS= read -r -d '' jar; do classpath+=":$jar"; done < <(find "$runtime_dir/libraries" -name '*.jar' -print0)
mkdir -p "$runtime_dir/classes"
javac -cp "$classpath" -d "$runtime_dir/classes" "$root_dir/tools/worldgen_oracle/InProcessWorldgenOracle.java"
(cd "$runtime_dir" && java -cp "$runtime_dir/classes:$classpath" InProcessWorldgenOracle "$seed")
