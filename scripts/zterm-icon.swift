// zterm's icon: a terminal's dark screen with its prompt, on the macOS icon
// grid (an 824-point squircle on a 1024 canvas, the rest its shadow's).
// swift scripts/zterm-icon.swift scripts/zterm.icns
import AppKit

let output = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "zterm.icns")

func render(_ size: Int) -> Data {
    let scale = CGFloat(size) / 1024
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let cx = CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    cx.scaleBy(x: scale, y: scale)
    // CoreGraphics runs bottom-up; the design is top-down.
    cx.translateBy(x: 0, y: 1024)
    cx.scaleBy(x: 1, y: -1)

    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    let shape = CGPath(roundedRect: body, cornerWidth: 185, cornerHeight: 185, transform: nil)

    // The grid's drop shadow.
    cx.saveGState()
    cx.setShadow(offset: CGSize(width: 0, height: 10), blur: 28, color: CGColor(gray: 0, alpha: 0.35))
    cx.addPath(shape)
    cx.setFillColor(CGColor(gray: 0.08, alpha: 1))
    cx.fillPath()
    cx.restoreGState()

    // The screen: near black, lighter at the top.
    cx.saveGState()
    cx.addPath(shape)
    cx.clip()
    let screen = CGGradient(
        colorsSpace: space,
        colors: [
            CGColor(srgbRed: 0.20, green: 0.22, blue: 0.25, alpha: 1),
            CGColor(srgbRed: 0.07, green: 0.08, blue: 0.09, alpha: 1),
        ] as CFArray,
        locations: [0, 1])!
    cx.drawLinearGradient(screen, start: CGPoint(x: 512, y: 100), end: CGPoint(x: 512, y: 924), options: [])
    cx.restoreGState()

    // A hairline rim, as glass catches light at its edge.
    cx.saveGState()
    cx.addPath(CGPath(roundedRect: body.insetBy(dx: 2, dy: 2), cornerWidth: 183, cornerHeight: 183, transform: nil))
    cx.setStrokeColor(CGColor(gray: 1, alpha: 0.14))
    cx.setLineWidth(4)
    cx.strokePath()
    cx.restoreGState()

    // The prompt: `>` and the cursor `_`.
    let ink = CGColor(srgbRed: 0.91, green: 0.93, blue: 0.92, alpha: 1)
    cx.setStrokeColor(ink)
    cx.setLineWidth(56)
    cx.setLineCap(.round)
    cx.setLineJoin(.round)
    cx.move(to: CGPoint(x: 262, y: 318))
    cx.addLine(to: CGPoint(x: 406, y: 424))
    cx.addLine(to: CGPoint(x: 262, y: 530))
    cx.strokePath()
    cx.setFillColor(ink)
    cx.addPath(CGPath(roundedRect: CGRect(x: 458, y: 502, width: 210, height: 54), cornerWidth: 12, cornerHeight: 12, transform: nil))
    cx.fillPath()

    let image = cx.makeImage()!
    return NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])!
}

let iconset = FileManager.default.temporaryDirectory
    .appendingPathComponent(UUID().uuidString)
    .appendingPathComponent("zterm.iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    for factor in [1, 2] {
        let name = factor == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        try render(points * factor).write(to: iconset.appendingPathComponent(name))
    }
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", output.path]
try iconutil.run()
iconutil.waitUntilExit()
exit(iconutil.terminationStatus)
