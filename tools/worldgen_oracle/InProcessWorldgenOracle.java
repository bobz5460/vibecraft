import net.minecraft.core.HolderLookup;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.world.level.levelgen.DensityFunction;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import java.io.PrintStream;

/**
 * Deterministic, in-process stage oracle. It never creates a level, opens a
 * socket, schedules chunk tasks, or saves NBT. It evaluates the exact 26.2
 * registry-backed Overworld RandomState directly.
 */
public final class InProcessWorldgenOracle {
    private InProcessWorldgenOracle() {}

    public static void main(String[] args) {
        long seed = args.length == 0 ? 42L : Long.parseLong(args[0]);
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        PrintStream out = Bootstrap.STDOUT;
        HolderLookup.Provider registries = VanillaRegistries.createLookup();
        RandomState state = RandomState.create(registries, NoiseGeneratorSettings.OVERWORLD, seed);

        int[][] positions = {{0, -64, 0}, {160, 63, 160}, {-160, 0, -160}, {176, 96, 160}};
        out.println("{\"schema\":1,\"minecraft_version\":\"26.2\",\"seed\":" + seed + ",\"samples\":[");
        for (int index = 0; index < positions.length; index++) {
            int[] p = positions[index];
            DensityFunction.FunctionContext context = new DensityFunction.SinglePointContext(p[0], p[1], p[2]);
            double density = state.router().finalDensity().compute(context);
            double preliminarySurface = state.router().preliminarySurfaceLevel().compute(context);
            out.printf("  {\"x\":%d,\"y\":%d,\"z\":%d,\"final_density\":%s,\"preliminary_surface\":%s}%s%n",
                p[0], p[1], p[2], Double.toHexString(density), Double.toHexString(preliminarySurface),
                index + 1 == positions.length ? "" : ",");
        }
        out.println("]}");
    }
}
