// Make zTerm.app what opens shell scripts and ssh:// and x-man-page:// links,
// as Terminal does: swift scripts/zterm-defaults.swift /Applications/zTerm.app
import AppKit
import UniformTypeIdentifiers

let app = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "/Applications/zTerm.app")
let workspace = NSWorkspace.shared
let group = DispatchGroup()
var failed = false

func done(_ what: String) -> (Error?) -> Void {
    group.enter()
    return { error in
        if let error {
            failed = true
            print("\(what): \(error.localizedDescription)")
        } else {
            print("\(what): zTerm")
        }
        group.leave()
    }
}

for scheme in ["ssh", "x-man-page"] {
    workspace.setDefaultApplication(at: app, toOpenURLsWithScheme: scheme, completion: done("\(scheme)://"))
}
// `.command` and `.tool` files (Terminal declares the type), and shell
// scripts by extension.
for identifier in ["com.apple.terminal.shell-script", "public.shell-script"] {
    if let type = UTType(identifier) {
        workspace.setDefaultApplication(at: app, toOpen: type, completion: done(identifier))
    }
}
group.notify(queue: .main) { exit(failed ? 1 : 0) }
dispatchMain()
