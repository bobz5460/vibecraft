import java.io.DataInputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.Tag;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.RegionFile;
import net.minecraft.world.level.chunk.storage.RegionStorageInfo;
import net.minecraft.util.SimpleBitStorage;

/**
 * Exports one fully generated Java chunk from an Anvil region file.
 *
 * This deliberately uses the target server's own RegionFile and NBT classes,
 * so the fixture is a Java-authored terrain/block-state oracle. Stage-level
 * probes belong in a later in-process exporter; this is the first stable
 * boundary that needs no decompiler-only API access.
 */
public final class WorldgenFixtureExporter {
    private WorldgenFixtureExporter() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 4) {
            throw new IllegalArgumentException(
                "usage: WorldgenFixtureExporter <region-dir> <chunk-x> <chunk-z> <output.nbt>"
            );
        }

        Path regionDir = Path.of(args[0]);
        int chunkX = Integer.parseInt(args[1]);
        int chunkZ = Integer.parseInt(args[2]);
        Path output = Path.of(args[3]);
        int regionX = Math.floorDiv(chunkX, 32);
        int regionZ = Math.floorDiv(chunkZ, 32);
        Path regionPath = regionDir.resolve("r." + regionX + "." + regionZ + ".mca");
        ChunkPos chunkPos = new ChunkPos(chunkX, chunkZ);

        Files.createDirectories(output.getParent());
        // RegionFile uses this only for diagnostics/JFR metadata while reading.
        // Avoid Level.OVERWORLD: loading Level initializes registries, which is
        // not valid in this standalone post-server export process.
        RegionStorageInfo storage = new RegionStorageInfo("worldgen-oracle", null, "chunk");
        try (RegionFile region = new RegionFile(storage, regionPath, regionDir, false);
             DataInputStream input = requireChunk(region, chunkPos)) {
            CompoundTag chunk = NbtIo.read(input);
            // Save timing, asynchronous lighting, and scheduled tick queues
            // do not describe terrain generation and are not byte-stable
            // across equivalent server captures. Keep block states, biome
            // palettes, heightmaps, structures, and block entities intact.
            chunk.remove("LastUpdate");
            chunk.remove("InhabitedTime");
            chunk.remove("isLightOn");
            chunk.remove("block_ticks");
            chunk.remove("fluid_ticks");
            chunk.remove("PostProcessing");
            for (Tag sectionTag : chunk.getListOrEmpty("sections")) {
                if (sectionTag instanceof CompoundTag section) {
                    section.remove("BlockLight");
                    section.remove("SkyLight");
                }
            }
            NbtIo.writeCompressed(chunk, output);
            String blockStateSha256 = blockStateSha256(chunk);
            String surfaceSummary = surfaceSummary(chunk);
            System.out.printf(
                "{\"chunk_x\":%d,\"chunk_z\":%d,\"status\":\"%s\",\"data_version\":%d,\"sections\":%d,\"block_state_sha256\":\"%s\",%s}%n",
                chunkX,
                chunkZ,
                jsonEscape(chunk.getStringOr("Status", "unknown")),
                chunk.getIntOr("DataVersion", -1),
                chunk.getListOrEmpty("sections").size(),
                blockStateSha256,
                surfaceSummary
            );
        }
    }

    private static String surfaceSummary(CompoundTag chunk) {
        Map<Integer, String[]> decodedSections = new HashMap<>();
        for (Tag tag : chunk.getListOrEmpty("sections")) {
            if (!(tag instanceof CompoundTag section)) continue;
            int sectionY = section.getByteOr("Y", (byte) 0);
            CompoundTag blockStates = section.getCompoundOrEmpty("block_states");
            ListTagView palette = new ListTagView(blockStates.getListOrEmpty("palette"));
            if (palette.size() == 0) continue;
            long[] data = blockStates.getLongArray("data").orElse(null);
            int bits = Math.max(4, ceilLog2(palette.size()));
            SimpleBitStorage storage = data == null ? null : new SimpleBitStorage(bits, 4096, data);
            String[] names = new String[4096];
            for (int index = 0; index < names.length; index++) {
                names[index] = palette.get(storage == null ? 0 : storage.get(index))
                    .getStringOr("Name", "minecraft:air");
            }
            decodedSections.put(sectionY, names);
        }

        int min = Integer.MAX_VALUE;
        int max = Integer.MIN_VALUE;
        long sum = 0;
        TreeMap<String, Integer> tops = new TreeMap<>();
        for (int z = 0; z < 16; z++) {
            for (int x = 0; x < 16; x++) {
                int topY = -64;
                String topName = "minecraft:air";
                for (int y = 319; y >= -64; y--) {
                    String[] section = decodedSections.get(Math.floorDiv(y, 16));
                    if (section == null) continue;
                    String name = section[((y & 15) << 8) | (z << 4) | x];
                    if (!name.equals("minecraft:air") && !name.equals("minecraft:cave_air") && !name.equals("minecraft:void_air")) {
                        topY = y;
                        topName = name;
                        break;
                    }
                }
                min = Math.min(min, topY);
                max = Math.max(max, topY);
                sum += topY;
                tops.merge(topName, 1, Integer::sum);
            }
        }
        StringBuilder counts = new StringBuilder();
        for (Map.Entry<String, Integer> entry : tops.entrySet()) {
            if (counts.length() != 0) counts.append(',');
            counts.append('\"').append(jsonEscape(entry.getKey())).append("\":").append(entry.getValue());
        }
        return "\"top_y_min\":" + min + ",\"top_y_max\":" + max +
            ",\"top_y_avg\":" + (sum / 256.0) + ",\"top_blocks\":{" + counts + "}";
    }

    private static DataInputStream requireChunk(RegionFile region, ChunkPos pos) throws Exception {
        DataInputStream input = region.getChunkDataInputStream(pos);
        if (input == null) {
            throw new IllegalStateException("chunk was not generated: " + pos);
        }
        return input;
    }

    private static String jsonEscape(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }

    /** Hash the decoded 16³ block states, independent of palette ordering or NBT serialization. */
    private static String blockStateSha256(CompoundTag chunk) throws Exception {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        List<CompoundTag> sections = new ArrayList<>();
        for (Tag tag : chunk.getListOrEmpty("sections")) {
            if (tag instanceof CompoundTag section) {
                sections.add(section);
            }
        }
        sections.sort(Comparator.comparingInt(section -> section.getByteOr("Y", (byte) 0)));

        for (CompoundTag section : sections) {
            int y = section.getByteOr("Y", (byte) 0);
            CompoundTag blockStates = section.getCompoundOrEmpty("block_states");
            ListTagView palette = new ListTagView(blockStates.getListOrEmpty("palette"));
            int paletteSize = palette.size();
            long[] data = blockStates.getLongArray("data").orElse(null);
            int bits = Math.max(4, ceilLog2(paletteSize));
            SimpleBitStorage storage = data == null ? null : new SimpleBitStorage(bits, 4096, data);
            update(digest, "section=" + y + "\n");
            for (int index = 0; index < 4096; index++) {
                int paletteIndex = storage == null ? 0 : storage.get(index);
                if (paletteIndex >= paletteSize) {
                    throw new IllegalStateException("invalid block-state palette index " + paletteIndex);
                }
                update(digest, canonicalState(palette.get(paletteIndex)) + "\n");
            }
        }
        return HexFormat.of().formatHex(digest.digest());
    }

    private static int ceilLog2(int value) {
        return value <= 1 ? 0 : 32 - Integer.numberOfLeadingZeros(value - 1);
    }

    private static String canonicalState(CompoundTag state) {
        return state.getStringOr("Name", "minecraft:air") + state.getCompound("Properties")
            .map(properties -> properties.toString())
            .orElse("");
    }

    private static void update(MessageDigest digest, String value) {
        digest.update(value.getBytes(StandardCharsets.UTF_8));
    }

    /** Avoid depending on an unstable ListTag generic API in the exporter. */
    private record ListTagView(net.minecraft.nbt.ListTag list) {
        int size() { return list.size(); }
        CompoundTag get(int index) { return list.getCompound(index).orElseThrow(); }
    }
}
