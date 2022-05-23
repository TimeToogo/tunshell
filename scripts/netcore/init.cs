// === TUNSHELL C# SCRIPT ===
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Net;
using System.IO;
using System.Threading.Tasks;
using System.Diagnostics;
using System.Security.AccessControl;

namespace Tunshell
{
    public static class Init
    {
        private readonly static Dictionary<OSPlatform, Dictionary<Architecture, string>> Targets = new Dictionary<OSPlatform, Dictionary<Architecture, string>>() {
            {
                OSPlatform.Windows,
                new Dictionary<Architecture, string>{
                    {Architecture.X64, "x86_64-pc-windows-msvc.exe"},
                    {Architecture.X86, "i686-pc-windows-msvc.exe"},
                }
            },
            {
                OSPlatform.OSX,
                new Dictionary<Architecture, string>{
                    {Architecture.X64, "x86_64-apple-darwin"},
                    {Architecture.Arm64, "aarch64-apple-darwin"},
                }
            },
            {
                OSPlatform.Linux,
                new Dictionary<Architecture, string>{
                    {Architecture.X64, "x86_64-unknown-linux-musl"},
                    {Architecture.X86, "i686-unknown-linux-musl"},
                    {Architecture.Arm, "armv7-unknown-linux-musleabihf"},
                    {Architecture.Arm64, "aarch64-unknown-linux-musl"},
                }
            },
        };

        static void Main(string[] args)
        {
            Console.WriteLine("Installing client...");
            Run(args).Wait();
        }

        private async static Task Run(string[] args)
        {
            var clientPath = Path.GetTempFileName();
            var webClient = new WebClient();

            await webClient.DownloadFileTaskAsync($"https://artifacts.tunshell.com/client-{GetTarget()}", clientPath);

            if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
                Process.Start(new ProcessStartInfo()
                {
                    FileName = "chmod",
                    Arguments = "+x " + clientPath
                }).WaitForExit();
            }

            var process = Process.Start(new ProcessStartInfo()
            {
                FileName = clientPath,
                Arguments = string.Join(" ", args),
                UseShellExecute = false,
                RedirectStandardError = true,
                RedirectStandardOutput = true
            });

            process.OutputDataReceived += (sender, e) =>
            {
                Console.WriteLine(e.Data);
            };

            process.ErrorDataReceived += (sender, e) =>
            {
                Console.Error.WriteLine(e.Data);
            };

            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
            process.WaitForExit();

            File.Delete(clientPath);
        }

        private static string GetTarget()
        {
            Dictionary<Architecture, string> targets = null;

            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
                targets = Targets[OSPlatform.Windows];
            }
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
            {
                targets = Targets[OSPlatform.OSX];
            }
            else if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
            {
                targets = Targets[OSPlatform.Linux];
            }
            else
            {
                throw new Exception($"Unsupported platform");
            }

            var cpu = RuntimeInformation.ProcessArchitecture;

            if (!targets.ContainsKey(cpu))
            {
                throw new Exception($"Unsupported CPU architecture: {cpu.ToString()}");
            }

            return targets[cpu];
        }
    }
}