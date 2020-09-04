import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.Objects;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public class init {

  public enum Os {
    LINUX,
    OSX,
    WINDOWS;

    public static Os detect() {
      return from(System.getProperty("os.name").toLowerCase());
    }

    public static Os from(String name) {
      switch (name) {
        case "linux":
          return Os.LINUX;
        case "os x":
        case "mac":
        case "mac os x":
          return Os.OSX;
        case "windows":
          return Os.WINDOWS;
      }
      throw new RuntimeException(String.format("Unsupported platform: %s", name));
    }
  }

  public enum Arch {
    X86_64,
    I686,
    ARM,
    ARM64;

    public static Arch detect() {
      return from(System.getProperty("os.arch").toLowerCase());
    }

    public static Arch from(String name) {
      switch (name) {
        case "x86_64":
        case "amd64":
          return Arch.X86_64;
        case "i686":
          return Arch.I686;
        case "arm":
          return Arch.ARM;
        case "arm64":
          return Arch.ARM64;
      }
      throw new RuntimeException(String.format("Unsupported CPU architecture: %s", name));
    }
  }

  public static class OsArch {
    private final Os os;
    private final Arch arch;

    public OsArch(Os os, Arch arch) {
      this.os = Objects.requireNonNull(os);
      this.arch = Objects.requireNonNull(arch);
    }

    @Override
    public boolean equals(Object o) {
      if (this == o) {
        return true;
      }
      if (o == null || getClass() != o.getClass()) {
        return false;
      }
      OsArch osArch = (OsArch) o;
      if (os != osArch.os) {
        return false;
      }
      return arch == osArch.arch;
    }

    @Override
    public int hashCode() {
      int result = os.hashCode();
      result = 31 * result + arch.hashCode();
      return result;
    }
  }

  private static final Map<OsArch, String> SUPPORTED_TARGETS;

  static {
    final Map<OsArch, String> targets = new HashMap<>();
    targets.put(new OsArch(Os.LINUX, Arch.X86_64), "x86_64-unknown-linux-musl");
    targets.put(new OsArch(Os.LINUX, Arch.I686), "i686-unknown-linux-musl");
    targets.put(new OsArch(Os.LINUX, Arch.ARM), "armv7-unknown-linux-musleabihf");
    targets.put(new OsArch(Os.LINUX, Arch.ARM64), "aarch64-unknown-linux-musl");
    targets.put(new OsArch(Os.OSX, Arch.X86_64), "x86_64-apple-darwin");
    targets.put(new OsArch(Os.WINDOWS, Arch.X86_64), "x86_64-pc-windows-msvc.exe");
    targets.put(new OsArch(Os.WINDOWS, Arch.I686), "i686-pc-windows-msvc.exe");
    SUPPORTED_TARGETS = Collections.unmodifiableMap(targets);
  }

  public static String getTarget(Os os, Arch arch) {
    final String target = SUPPORTED_TARGETS.get(new OsArch(os, arch));
    if (target == null) {
      throw new RuntimeException(
        String.format("Unsupported platform / CPU architecture: %s / %s", os, arch));
    }
    return target;
  }

  public static String getTarget() {
    return getTarget(Os.detect(), Arch.detect());
  }

  public static void main(String[] args) throws IOException, InterruptedException {
    System.out.println("Installing client...");

    final File clientFile = File.createTempFile("tunshell-client", null);
    final String url = String.format("https://artifacts.tunshell.com/client-%s", getTarget());
    try (final InputStream in = new URL(url).openStream()) {
      Files.copy(in, clientFile.toPath(), StandardCopyOption.REPLACE_EXISTING);
    }
    if (!clientFile.setExecutable(true)) {
      throw new RuntimeException(String.format("Unable to mark '%s' as executable", clientFile));
    }

    new ProcessBuilder(
      Stream.concat(Stream.of(clientFile.getAbsolutePath()), Stream.of(args))
        .collect(Collectors.toList()))
      .inheritIO()
      .start()
      .waitFor();
  }
}
