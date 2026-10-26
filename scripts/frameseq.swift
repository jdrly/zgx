import CoreImage
import CoreMedia
import Foundation
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers
// frameseq <pid> <delay> <seconds> [dir]: the sequence of distinct images a
// process's window shows, as the window server composites it (ScreenCaptureKit,
// as framepace.swift captures it). A frame counts when its pixels differ from
// the one before; it is a *revisit* when they equal one of the few distinct
// images shown just before (A, B, A): content older than the last frame shown
// again, as a surface presented with stale or half-updated pixels would show.
// A correct app's frames move forward, so its revisits are none (an
// animation that returns to an earlier state exactly can revisit, a few times
// a second at most). With `dir`, the first 120 changed frames are written
// there as PNGs, named by their order and short hash. JSON on stdout.
let a = CommandLine.arguments
let pid = pid_t(a[1])!, delay = Double(a[2])!, seconds = Double(a[3])!
let dir = a.count > 4 ? URL(fileURLWithPath: a[4]) : nil
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
/// How many distinct images back a repeat counts as a revisit.
let lookback = 4

final class Recorder: NSObject, SCStreamOutput {
  var changes: [(Double, UInt64)] = []
  var revisits: [(Int, Int)] = []  // (change index, how many images back)
  var recent: [UInt64] = []
  var recording = false
  var last: UInt64 = 0
  var saved = 0
  let lock = NSLock()
  let context = CIContext()
  func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen,
          let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
          let raw = attachments.first?[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
          let display = attachments.first?[.displayTime] as? UInt64,
          let pixels = CMSampleBufferGetImageBuffer(buffer)
    else { return }
    CVPixelBufferLockBaseAddress(pixels, .readOnly)
    guard let base = CVPixelBufferGetBaseAddress(pixels) else {
      CVPixelBufferUnlockBaseAddress(pixels, .readOnly); return
    }
    let (width, height, stride) = (CVPixelBufferGetWidth(pixels), CVPixelBufferGetHeight(pixels), CVPixelBufferGetBytesPerRow(pixels))
    // Every pixel: a revisit must be the same image, not a near one.
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    for y in 0..<height {
      let row = base.advanced(by: y * stride).assumingMemoryBound(to: UInt32.self)
      for x in 0..<width {
        hash = (hash ^ UInt64(row[x])) &* 0x100_0000_01b3
      }
    }
    CVPixelBufferUnlockBaseAddress(pixels, .readOnly)
    lock.withLock {
      defer { last = hash }
      guard recording, hash != last else { return }
      let index = changes.count
      changes.append((Double(display) * Double(tb.numer) / Double(tb.denom) / 1e9, hash))
      // `recent` holds the distinct images before this one, newest last;
      // the one just before is `last`, so a repeat of it is no change.
      // How many distinct images back this one was last shown, if lately.
      let before = Array(recent.dropLast())
      if let at = before.lastIndex(of: hash) {
        revisits.append((index, before.count - at + 1))
      }
      recent.append(hash)
      if recent.count > lookback + 1 { recent.removeFirst() }
      if let dir, saved < 120 {
        saved += 1
        let image = CIImage(cvPixelBuffer: pixels)
        if let cg = context.createCGImage(image, from: image.extent) {
          let url = dir.appendingPathComponent(String(format: "%03d-%08llx.png", index, hash & 0xffff_ffff))
          if let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) {
            CGImageDestinationAddImage(dest, cg, nil)
            CGImageDestinationFinalize(dest)
          }
        }
      }
    }
  }
}

func fail(_ message: String) -> Never {
  print("{\"error\":\"\(message)\"}"); exit(1)
}

let recorder = Recorder()
if let dir { try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true) }
Task {
  do {
    let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
    guard let window = content.windows
      .filter({ $0.owningApplication?.processID == pid })
      .max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height })
    else { fail("no window") }
    let rate = NSScreen.screens.map(\.maximumFramesPerSecond).max() ?? 60
    let config = SCStreamConfiguration()
    let titleBar: CGFloat = 32
    config.sourceRect = CGRect(x: 0, y: titleBar, width: window.frame.width, height: window.frame.height - titleBar)
    config.width = Int(window.frame.width / 2)
    config.height = Int((window.frame.height - titleBar) / 2)
    config.pixelFormat = kCVPixelFormatType_32BGRA
    config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(rate))
    config.queueDepth = 8
    config.showsCursor = false
    let stream = SCStream(filter: SCContentFilter(desktopIndependentWindow: window), configuration: config, delegate: nil)
    try stream.addStreamOutput(recorder, type: .screen, sampleHandlerQueue: DispatchQueue(label: "frames"))
    try await stream.startCapture()
    try await Task.sleep(nanoseconds: UInt64(delay * 1e9))
    recorder.lock.withLock { recorder.recording = true }
    try await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
    let (changes, revisits) = recorder.lock.withLock {
      recorder.recording = false
      return (recorder.changes, recorder.revisits)
    }
    try await stream.stopCapture()
    let back2 = revisits.filter { $0.1 == 2 }.count
    let first = revisits.prefix(12).map { "\($0.0):\($0.1)" }.joined(separator: ",")
    print(String(format: "{\"display_hz\":%d,\"changes\":%d,\"changes_per_second\":%.3f,\"revisits\":%d,\"revisit_fraction\":%.5f,\"revisits_two_back\":%d,\"first_revisits\":\"%@\"}",
      rate, changes.count, Double(changes.count) / seconds, revisits.count,
      changes.isEmpty ? 0 : Double(revisits.count) / Double(changes.count), back2, first))
    exit(0)
  } catch {
    fail("\(error)".replacingOccurrences(of: "\"", with: "'"))
  }
}
RunLoop.main.run()
