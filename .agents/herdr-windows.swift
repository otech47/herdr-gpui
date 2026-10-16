import CoreGraphics
import Foundation

// System Events sees no gpui window, so CGWindowList is the only lister that works.
let options: CGWindowListOption = [.optionAll]
guard let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    print("no window list")
    exit(1)
}
for window in list {
    guard window["kCGWindowOwnerName"] as? String == "Herdr" else { continue }
    guard (window["kCGWindowLayer"] as? Int) == 0 else { continue }
    let number = window["kCGWindowNumber"] as? Int ?? -1
    let onscreen = (window["kCGWindowIsOnscreen"] as? Bool) == true
    let title = window["kCGWindowName"] as? String ?? ""
    let bounds = window["kCGWindowBounds"] as? [String: Any] ?? [:]
    let x = bounds["X"] as? Int ?? 0
    let y = bounds["Y"] as? Int ?? 0
    let w = bounds["Width"] as? Int ?? 0
    let h = bounds["Height"] as? Int ?? 0
    print("\(number)\tonscreen=\(onscreen)\t\(w)x\(h)@\(x),\(y)\t\(title)")
}
