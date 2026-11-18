import AppKit
import SwiftUI

// MARK: - helper bridge

struct Swap {
    let path: String
    let state: String
}

struct Snapshot {
    var obeliskInstalled = false
    var obeliskApp = ""
    var install = ""
    var cooked = ""
    var prefix = ""
    var store = ""
    var running = false
    var swaps: [Swap] = []

    var hasInstall: Bool { !install.isEmpty }
}

enum Helper {
    static var path: String {
        Bundle.main.bundleURL
            .appendingPathComponent("Contents/MacOS/hebnix-macos-cli")
            .path
    }

    @discardableResult
    static func run(_ arguments: [String]) -> (status: Int32, out: String, err: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err
        do {
            try process.run()
        } catch {
            return (-1, "", "could not start helper: \(error.localizedDescription)")
        }
        process.waitUntilExit()
        let output = String(data: out.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let errors = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return (process.terminationStatus, output, errors.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    static func snapshot() -> Snapshot {
        var result = Snapshot()
        let status = run(["status"])
        for line in status.out.split(separator: "\n") {
            let parts = line.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            guard parts.count == 2 else { continue }
            let key = String(parts[0])
            let value = String(parts[1])
            switch key {
            case "obelisk_installed": result.obeliskInstalled = value == "true"
            case "obelisk_app": result.obeliskApp = value
            case "install": result.install = value
            case "cooked": result.cooked = value
            case "prefix": result.prefix = value
            case "store": result.store = value
            case "running": result.running = value == "true"
            default: break
            }
        }
        if result.hasInstall {
            let list = run(["swaps"])
            result.swaps = list.out
                .split(separator: "\n")
                .compactMap { line -> Swap? in
                    let parts = line.split(separator: "\t", maxSplits: 1)
                    guard parts.count == 2 else { return nil }
                    return Swap(path: String(parts[0]), state: String(parts[1]))
                }
        }
        return result
    }
}

// MARK: - app

@main
struct HebnixMacApp: App {
    var body: some Scene {
        WindowGroup("Hebnix for Mac") {
            ContentView()
                .frame(minWidth: 620, minHeight: 560)
        }
        .windowResizability(.contentMinSize)
    }
}

struct ContentView: View {
    @State private var snapshot = Snapshot()
    @State private var message = ""
    @State private var isError = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            if snapshot.hasInstall {
                installCard
                swapsCard
            } else {
                missingCard
            }
            if !message.isEmpty {
                Text(message)
                    .font(.callout)
                    .foregroundStyle(isError ? Color.red : Color(nsColor: .secondaryLabelColor))
                    .textSelection(.enabled)
            }
            Spacer(minLength: 0)
            footer
        }
        .padding(20)
        .onAppear(perform: refresh)
    }

    private var header: some View {
        HStack(spacing: 12) {
            Image(systemName: "bolt.horizontal.circle.fill")
                .font(.system(size: 30))
                .foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 2) {
                Text("Hebnix for Mac").font(.title2.weight(.semibold))
                Text("Test build — file swaps through Obelisk")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button("Refresh", action: refresh)
        }
    }

    private var installCard: some View {
        GroupBox("Rocket League") {
            VStack(alignment: .leading, spacing: 6) {
                row("Install", snapshot.install)
                row("Packages", snapshot.cooked)
                if !snapshot.prefix.isEmpty {
                    row("Prefix", snapshot.prefix)
                }
                row("Store", snapshot.store)
                row("Game running", snapshot.running ? "yes — close it before swapping" : "no")
            }
            .padding(.top, 4)
        }
    }

    private var swapsCard: some View {
        GroupBox("Swaps") {
            VStack(alignment: .leading, spacing: 10) {
                if snapshot.swaps.isEmpty {
                    Text("No swaps yet. Applied files are backed up and can be restored.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(snapshot.swaps, id: \.path) { swap in
                        HStack(spacing: 8) {
                            Text((swap.path as NSString).lastPathComponent)
                                .font(.callout.monospaced())
                            Text(swap.state)
                                .font(.caption)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(badgeColor(swap.state).opacity(0.16))
                                .foregroundStyle(badgeColor(swap.state))
                                .clipShape(Capsule())
                            Spacer()
                            Button("Restore") { restore(swap.path) }
                                .buttonStyle(.link)
                        }
                    }
                }
                Divider()
                HStack {
                    Button("Swap in a .upk…", action: pickAndApply)
                        .disabled(snapshot.running)
                    Button("Restore all") { restore(nil) }
                        .disabled(snapshot.running || snapshot.swaps.isEmpty)
                    Spacer()
                    Button("Reveal game folder") {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: snapshot.cooked)])
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    private var missingCard: some View {
        GroupBox("Rocket League not found") {
            VStack(alignment: .leading, spacing: 8) {
                Text(
                    snapshot.obeliskInstalled
                        ? "Obelisk is installed. Install Rocket League in Obelisk, launch it once, then hit Refresh."
                        : "Obelisk was not found in /Applications. Install it from getobelisk.app, add Rocket League, then hit Refresh."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                if snapshot.obeliskInstalled {
                    Button("Open Obelisk") { run(["open"], success: "Obelisk is coming forward.") }
                }
            }
            .padding(.top, 4)
        }
    }

    private var footer: some View {
        HStack {
            Text("Swaps change files only. Nothing is injected, and Obelisk still launches the game.")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Button("Open Obelisk") { run(["open"], success: "Obelisk is coming forward.") }
                .disabled(!snapshot.obeliskInstalled)
        }
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .frame(width: 96, alignment: .leading)
            Text(value.isEmpty ? "—" : value)
                .font(.caption.monospaced())
                .textSelection(.enabled)
                .lineLimit(2)
                .truncationMode(.middle)
        }
    }

    private func badgeColor(_ state: String) -> Color {
        switch state {
        case "swapped": return .green
        case "changed": return .orange
        default: return Color(nsColor: .secondaryLabelColor)
        }
    }

    private func refresh() {
        snapshot = Helper.snapshot()
        if !snapshot.hasInstall {
            isError = false
            message = ""
        }
    }

    private func pickAndApply() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.message = "Pick the .upk to swap in. Its name must match the file it replaces."
        guard panel.runModal() == .OK, let url = panel.url else { return }
        run(["apply", url.lastPathComponent, url.path], success: "Applied \(url.lastPathComponent).")
    }

    private func restore(_ relative: String?) {
        if let relative {
            run(["restore", relative], success: "Restored \(relative).")
        } else {
            run(["restore-all"], success: "Restored everything.")
        }
    }

    private func run(_ arguments: [String], success: String) {
        let result = Helper.run(arguments)
        if result.status == 0 {
            isError = false
            message = success
        } else {
            isError = true
            message = result.err.isEmpty ? "failed with exit code \(result.status)" : result.err
        }
        refresh()
    }
}
