import java.io.File;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;

public class init {
    public static void main(String[] args) throws Exception {
        System.out.println("Installing client...");
        var clientFile = File.createTempFile("tunshell-client", "",
                new File(System.getProperty("java.io.tmpdir")));

        var url = String.format("https://artifacts.tunshell.com/client-%s", getTarget());
        try (var in = new URL(url).openStream()) {
            Files.copy(in, Paths.get(clientFile.getPath()), StandardCopyOption.REPLACE_EXISTING);
        }

        clientFile.setExecutable(true);

        var command = new ArrayList<String>() {
            {
                add(clientFile.getAbsolutePath());
                addAll(Arrays.asList(args));
            }
        };
        var pb = new ProcessBuilder(command);
        pb.inheritIO();
        var process = pb.start();
        process.waitFor();
    }

    private static String getTarget() throws Exception {
        var targets = new HashMap<String, HashMap<String, String>>() {
            {
                put("linux", new HashMap<String, String>() {
                    {
                        put("x86_64", "x86_64-unknown-linux-musl");
                        put("i686", "i686-unknown-linux-musl");
                        put("arm", "armv7-unknown-linux-musleabihf");
                        put("arm64", "aarch64-unknown-linux-musl");
                    }
                });
                put("osx", new HashMap<String, String>() {
                    {
                        put("x86_64", "x86_64-apple-darwin");
                    }
                });
                put("windows", new HashMap<String, String>() {
                    {
                        put("x86_64", "x86_64-pc-windows-msvc.exe");
                        put("i686", "i686-pc-windows-msvc.exe");
                    }
                });
            }
        };

        var os = System.getProperty("os.name").toLowerCase();
        HashMap<String, String> osTargets = null;

        if (os.contains("linux")) {
            osTargets = targets.get("linux");
        } else if (os.contains("mac") || os.contains("os x")) {
            osTargets = targets.get("osx");
        } else if (os.contains("windows")) {
            osTargets = targets.get("windows");
        } else {
            throw new Exception(String.format("Unsupported platform: %s", os));
        }

        var cpu = System.getProperty("os.arch");

        if (!osTargets.containsKey(cpu)) {
            throw new Exception(String.format("Unsupported CPU architecture: %s", cpu));
        }

        return osTargets.get(cpu);
    }
}
