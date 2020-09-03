import java.io.File;
import java.io.InputStream;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class init {
    public static void main(String[] args) throws Exception {
        System.out.println("Installing client...");
        File clientFile = File.createTempFile("tunshell-client", "",
                new File(System.getProperty("java.io.tmpdir")));

        String url = String.format("https://artifacts.tunshell.com/client-%s", getTarget());
        try (InputStream in = new URL(url).openStream()) {
            Files.copy(in, Paths.get(clientFile.getPath()), StandardCopyOption.REPLACE_EXISTING);
        }

        clientFile.setExecutable(true);

        List<String> command = new ArrayList<String>() {
            {
                add(clientFile.getAbsolutePath());
                addAll(Arrays.asList(args));
            }
        };
        ProcessBuilder pb = new ProcessBuilder(command);
        pb.inheritIO();
        Process process = pb.start();
        process.waitFor();
    }

    private static String getTarget() throws Exception {
        Map<String, Map<String, String>> targets = new HashMap<String, Map<String, String>>() {
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

        String os = System.getProperty("os.name").toLowerCase();
        Map<String, String> osTargets = null;

        if (os.contains("linux")) {
            osTargets = targets.get("linux");
        } else if (os.contains("mac") || os.contains("os x")) {
            osTargets = targets.get("osx");
        } else if (os.contains("windows")) {
            osTargets = targets.get("windows");
        } else {
            throw new Exception(String.format("Unsupported platform: %s", os));
        }

        String cpu = System.getProperty("os.arch");

        if (!osTargets.containsKey(cpu)) {
            throw new Exception(String.format("Unsupported CPU architecture: %s", cpu));
        }

        return osTargets.get(cpu);
    }
}
