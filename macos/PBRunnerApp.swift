import SwiftUI
import AppKit
import Metal

@MainActor
final class RunnerModel: ObservableObject {
    @Published var output = "PB belum diverifikasi bisa masuk pertandingan.\n"
    @Published var busy = false
    @Published var diagnosticLogging = false
    @Published var networkLogging = false
    @Published var status = "Siap untuk pengujian pertama"
    private var active: Process?
    private var activeState: URL?

    private var resources: URL { Bundle.main.resourceURL! }
    var state: URL {
        if let value = Bundle.main.object(forInfoDictionaryKey: "PBStateDirectory") as? String {
            return URL(fileURLWithPath: value)
        }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("PB Runner", isDirectory: true)
    }
    private var wine: URL {
        let path = Bundle.main.object(forInfoDictionaryKey: "PBWineRelativePath") as? String
            ?? "runtime/Wine Devel.app/Contents/Resources/wine/bin/wine"
        return resources.appendingPathComponent(path)
    }
    var runtimeLabel: String {
        Bundle.main.object(forInfoDictionaryKey: "PBWineLabel") as? String ?? "Wine devel 11.18"
    }
    private var runner: URL { resources.appendingPathComponent("pb-runner") }

    init() {
        let devices = MTLCopyAllDevices()
        let names = devices.map { $0.name }
        let report: [String: Any] = ["timestamp": ISO8601DateFormatter().string(from: Date()),
            "host_arch": "arm64", "metal_devices": names,
            "metal_default": MTLCreateSystemDefaultDevice()?.name ?? "unavailable"]
        if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) {
            try? data.write(to: state.appendingPathComponent("gui-metal-probe.json"), options: .atomic)
        }
        append("Metal aplikasi: " + (names.isEmpty ? "tidak tersedia" : names.joined(separator: ", ")) + "\n")
    }


    func append(_ text: String) {
        output += text
        if output.count > 160_000 { output = String(output.suffix(120_000)) }
    }

    func perform(_ steps: [[String]], label: String, stateDirectory: URL? = nil) {
        guard !busy else { return }
        busy = true
        status = label
        let executable = runner
        let traceNetwork = networkLogging
        let debug = diagnosticLogging ? "-all,err+all,+mscoree,+loaddll,+seh,+module" : "-all,err+all"
        let selectedState = stateDirectory ?? state
        activeState = selectedState
        let base = ["--state", selectedState.path, "--wine", wine.path, "--debug", debug]
        append("\n--- \(label) ---\n")
        DispatchQueue.global(qos: .userInitiated).async {
            var result: Int32 = 0
            for step in steps {
                let process = Process()
                let pipe = Pipe()
                process.executableURL = executable
                process.arguments = base + step
                var environment = ProcessInfo.processInfo.environment
                environment.removeValue(forKey: "WINE_MONO_TRACE")
                environment["MVK_CONFIG_LOG_LEVEL"] = "2"
                if traceNetwork {
                    environment["WINE_MONO_TRACE"] = "M:System.Net.WebClient:DownloadFileAsync,M:System.Net.WebClient:DownloadDataAsync,E:System.Net.WebException,E:System.IO.IOException"
                }
                process.environment = environment
                process.standardOutput = pipe
                process.standardError = pipe
                process.standardInput = FileHandle.nullDevice
                DispatchQueue.main.async { self.active = process }
                pipe.fileHandleForReading.readabilityHandler = { handle in
                    let data = handle.availableData
                    if !data.isEmpty {
                        let text = String(decoding: data, as: UTF8.self)
                        // Full traces are already captured by Rust. Avoid flooding
                        // SwiftUI/Accessibility while a startup trace is active.
                        let visible = text.components(separatedBy: "\n")
                            .filter { !$0.contains(":trace:") && !$0.contains(":fixme:") }
                            .joined(separator: "\n")
                        if !visible.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            DispatchQueue.main.async { self.append(visible) }
                        }
                    }
                }
                do {
                    try process.run()
                    process.waitUntilExit()
                    result = process.terminationStatus
                } catch {
                    result = 1
                    let message = "Gagal menjalankan runner: \(error.localizedDescription)\n"
                    DispatchQueue.main.async { self.append(message) }
                }
                pipe.fileHandleForReading.readabilityHandler = nil
                // Process/descendant lifetime is reported by the Rust core's result.json.
                if result != 0 { break }
            }
            let finalResult = result
            DispatchQueue.main.async {
                self.active = nil
                self.activeState = nil
                self.busy = false
                self.status = finalResult == 0
                    ? "Proses selesai. Keberhasilan PB tetap perlu diamati."
                    : "Proses berhenti dengan kode \(finalResult). Lihat log."
            }
        }
    }

    func prepare() {
        perform([["doctor"], ["--timeout", "120", "init"]], label: "Menyiapkan lingkungan PB mandiri")
    }

    func install() {
        let installer = resources.appendingPathComponent("installer/Install_POINTBLANK_20251111.exe")
        let alert = NSAlert()
        alert.messageText = "Pasang launcher Point Blank resmi?"
        alert.informativeText = "Installer diunduh dari situs resmi PB. Ikuti jendela installer yang terbuka; persetujuan lisensi dan login dilakukan oleh kamu."
        alert.addButton(withTitle: "Jalankan Installer")
        alert.addButton(withTitle: "Batal")
        if alert.runModal() == .alertFirstButtonReturn {
            perform([["--timeout", "120", "init"], ["install", installer.path]], label: "Menjalankan installer resmi PB")
        }
    }

    func probeD3D9() {
        let mono = wine.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("share/wine/mono")
        do {
            let version = try FileManager.default.contentsOfDirectory(at: mono, includingPropertiesForKeys: nil).sorted { $0.path < $1.path }.last!
            let source = resources.appendingPathComponent("D3D9Probe.cs")
            let executable = state.appendingPathComponent("D3D9Probe.exe")
            func zpath(_ p: String) -> String { "Z:" + p.replacingOccurrences(of: "/", with: "\\") }
            perform([
                ["--timeout", "30", "run", version.appendingPathComponent("lib/mono/4.5/mcs.exe").path, "-platform:x86", "-r:System.Windows.Forms", "-r:System.Drawing", "-out:" + zpath(executable.path), zpath(source.path)],
                ["--timeout", "30", "run", executable.path]
            ], label: "Menguji pembuatan perangkat D3D9 32-bit")
        } catch { append("Uji D3D9: \(error)\n") }
    }

    func probeD3D11() {
        let mono = wine.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("share/wine/mono")
        do {
            let version = try FileManager.default.contentsOfDirectory(at: mono, includingPropertiesForKeys: nil).sorted { $0.path < $1.path }.last!
            let source = resources.appendingPathComponent("D3D11Probe.cs")
            let executable = state.appendingPathComponent("D3D11Probe.exe")
            func zpath(_ p: String) -> String { "Z:" + p.replacingOccurrences(of: "/", with: "\\") }
            perform([
                ["--timeout", "30", "run", version.appendingPathComponent("lib/mono/4.5/mcs.exe").path, "-platform:x86", "-r:System.Windows.Forms", "-r:System.Drawing", "-out:" + zpath(executable.path), zpath(source.path)],
                ["--timeout", "30", "run", executable.path]
            ], label: "Menguji pembuatan perangkat D3D11 32-bit")
        } catch { append("Uji D3D11: \(error)\n") }
    }

    func probeGPU() {
        guard !busy else { return }
        busy = true
        status = "Menguji Vulkan x86_64"
        let lib = wine.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("lib")
        let process = Process()
        process.executableURL = resources.appendingPathComponent("runtime-probe")
        process.arguments = [lib.appendingPathComponent("libvulkan.1.dylib").path, lib.appendingPathComponent("libodbc.dylib").path]
        var environment = ProcessInfo.processInfo.environment
        environment["DYLD_FALLBACK_LIBRARY_PATH"] = lib.path
        environment["VK_DRIVER_FILES"] = lib.appendingPathComponent("wine/x86_64-unix/vulkan/icd.d/MoltenVK_icd.json").path
        environment["VK_LOADER_DEBUG"] = "error,warn,driver"
        process.environment = environment
        let log = state.appendingPathComponent("gui-vulkan-probe.log")
        FileManager.default.createFile(atPath: log.path, contents: nil)
        guard let handle = try? FileHandle(forWritingTo: log) else { busy = false; return }
        process.standardOutput = handle
        process.standardError = handle
        process.terminationHandler = { p in
            try? handle.close()
            let result = (try? String(contentsOf: log, encoding: .utf8)) ?? "Log tidak tersedia"
            DispatchQueue.main.async {
                self.append("\nGPU probe exit \(p.terminationStatus):\n" + result + "\n")
                self.busy = false
                self.status = "Uji GPU selesai"
            }
        }
        do { try process.run() } catch { try? handle.close(); busy = false; append("GPU probe: \(error)\n") }
    }

    func inspectProcesses() {
        let process = Process()
        process.executableURL = runner
        process.arguments = ["--state", state.path, "--wine", wine.path, "processes"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try process.run()
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                let text = String(decoding: data, as: UTF8.self)
                DispatchQueue.main.async { self.append("\n--- Proses dalam lingkungan PB ---\n" + text + "\n") }
            } catch {
                DispatchQueue.main.async { self.append("Gagal membaca proses: \(error)\n") }
            }
        }
    }

    func configurePB() {
        perform([["run", "C:\\Zepetto\\PointBlank ID\\PBConfig.exe"]], label: "Membuka pengaturan grafis resmi PB")
    }

    func launch() {
        perform([["run"]], label: "Menjalankan Z-Launcher resmi")
    }

    func installDotNet() {
        let script = resources.appendingPathComponent("tools/winetricks")
        perform([["dotnet", script.path]], label: "Memasang Microsoft .NET 4.8 di lingkungan PB Runner")
    }

    func probeMono() {
        // A separate prefix keeps the architecture probe away from PB and native .NET experiments.
        let probeName = Bundle.main.object(forInfoDictionaryKey: "PBMonoProbeDirectory") as? String ?? "mono-probe-state"
        let probeState = state.deletingLastPathComponent().appendingPathComponent(probeName)
        let mono = wine.deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("share/wine/mono")
        do {
            let versions = try FileManager.default.contentsOfDirectory(at: mono, includingPropertiesForKeys: nil)
            guard let version = versions.sorted(by: { $0.path < $1.path }).last else {
                append("Wine Mono tidak ditemukan.\n"); return
            }
            try FileManager.default.createDirectory(at: probeState, withIntermediateDirectories: true)
            let source = probeState.appendingPathComponent("hello.cs")
            try "class P { static void Main() { System.Console.WriteLine(\"hello from \" + (System.IntPtr.Size * 8) + \"-bit\"); } }\n"
                .write(to: source, atomically: true, encoding: .utf8)
            let mcs = version.appendingPathComponent("lib/mono/4.5/mcs.exe").path
            func zpath(_ path: String) -> String { "Z:" + path.replacingOccurrences(of: "/", with: "\\") }
            let x64 = probeState.appendingPathComponent("hello64.exe").path
            let x86 = probeState.appendingPathComponent("hello32.exe").path
            perform([
                ["--timeout", "120", "init"],
                ["--timeout", "30", "run", mcs, "-platform:x64", "-out:" + zpath(x64), zpath(source.path)],
                ["--timeout", "30", "run", mcs, "-platform:x86", "-out:" + zpath(x86), zpath(source.path)],
                ["--timeout", "15", "run", x64],
                ["--timeout", "15", "run", x86]
            ], label: "Membandingkan program Mono 64-bit dan 32-bit tanpa PB", stateDirectory: probeState)
        } catch { append("Uji Mono gagal disiapkan: \(error.localizedDescription)\n") }
    }

    func stop() {
        let process = Process()
        process.executableURL = runner
        process.arguments = ["--state", (activeState ?? state).path, "--wine", wine.path, "stop"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        process.standardInput = FileHandle.nullDevice
        process.terminationHandler = { p in
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let text = String(decoding: data, as: UTF8.self)
            DispatchQueue.main.async {
                self.append(text)
                if p.terminationStatus == 0 { self.status = "Lingkungan PB dihentikan" }
            }
        }
        do { try process.run() }
        catch { append("Gagal menghentikan: \(error.localizedDescription)\n") }
    }

    func showLogs() {
        let directory = state.appendingPathComponent("logs", isDirectory: true)
        if FileManager.default.fileExists(atPath: directory.path) { NSWorkspace.shared.open(directory) }
        else { append("Belum ada log. Jalankan Siapkan terlebih dahulu.\n") }
    }
}

struct RunnerView: View {
    @StateObject private var model = RunnerModel()
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Image(systemName: "gamecontroller.fill").font(.system(size: 30)).foregroundStyle(.blue)
                VStack(alignment: .leading, spacing: 3) {
                    Text("PB Runner").font(.title.bold())
                    Text("Prototipe mandiri · mesin Rust").foregroundStyle(.secondary)
                }
                Spacer()
                Text("0.1.0").foregroundStyle(.secondary)
            }
            Text("Target: membuka PB resmi hingga bisa masuk pertandingan.")
            Text("Runtime uji: \(model.runtimeLabel)").font(.caption).foregroundStyle(.secondary)
            HStack(spacing: 10) {
                Button("1. Siapkan") { model.prepare() }.disabled(model.busy)
                Button("2. Pasang PB") { model.install() }.disabled(model.busy)
                Button(".NET 4.8") { model.installDotNet() }.disabled(model.busy)
                Button("Pengaturan PB") { model.configurePB() }.disabled(model.busy)
                Button("3. Jalankan PB") { model.launch() }.disabled(model.busy)
                Spacer()
                Button("Hentikan") { model.stop() }
                Button("Status Game") { model.inspectProcesses() }
                Button("Buka Log") { model.showLogs() }
            }
            HStack {
                if model.busy { ProgressView().controlSize(.small) }
                Text(model.status).font(.callout).foregroundStyle(.secondary)
            }
            Toggle("Log diagnostik .NET", isOn: $model.diagnosticLogging)
                .disabled(model.busy).font(.callout)
            Toggle("Log diagnostik unduhan", isOn: $model.networkLogging)
                .disabled(model.busy).font(.callout)
            Button("Uji D3D11 32-bit") { model.probeD3D11() }.disabled(model.busy)
            Button("Uji D3D9 32-bit") { model.probeD3D9() }.disabled(model.busy)
            Button("Uji GPU x86_64") { model.probeGPU() }.disabled(model.busy)
            Button("Uji Mono 32/64-bit tanpa PB") { model.probeMono() }.disabled(model.busy)
            ScrollView {
                Text(model.output)
                    .font(.system(size: 11, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                    .padding(12)
            }
            .background(Color(nsColor: .textBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            Text("Log tersimpan lokal. Keberhasilan proses belum berarti game berhasil dimainkan.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding(22)
        .frame(minWidth: 780, minHeight: 520)
    }
}

@main
struct PBRunnerApp: App {
    var body: some Scene {
        WindowGroup("PB Runner") { RunnerView() }
    }
}
