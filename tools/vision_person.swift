import Vision
import AppKit
import CoreImage

// High-throughput Vision person segmentation helper for insta-mask
// Usage: vision_person <input.jpg> <output.png> [--quality fast|balanced|accurate]
let args = CommandLine.arguments
guard args.count >= 3 else {
    fputs("usage: vision_person <input> <output> [--quality fast|balanced|accurate]\n", stderr)
    exit(1)
}
let inputPath = args[1]
let outputPath = args[2]
var qualityStr = "balanced"
if let idx = args.firstIndex(of: "--quality"), idx + 1 < args.count {
    qualityStr = args[idx + 1]
}
guard let ciImage = CIImage(contentsOf: URL(fileURLWithPath: inputPath)) else {
    fputs("failed to load image \(inputPath)\n", stderr)
    exit(1)
}
let request = VNGeneratePersonSegmentationRequest()
switch qualityStr {
case "fast": request.qualityLevel = .fast
case "accurate": request.qualityLevel = .accurate
default: request.qualityLevel = .balanced
}
request.outputPixelFormat = kCVPixelFormatType_OneComponent8

let handler = VNImageRequestHandler(ciImage: ciImage, orientation: .up, options: [:])
do {
    try handler.perform([request])
} catch {
    fputs("Vision perform failed: \(error)\n", stderr)
    exit(1)
}
guard let observation = request.results?.first else {
    fputs("no person segmentation result\n", stderr)
    exit(1)
}
let pixelBuffer = observation.pixelBuffer
let ciMask = CIImage(cvPixelBuffer: pixelBuffer)
let maskW = CVPixelBufferGetWidth(pixelBuffer)
let maskH = CVPixelBufferGetHeight(pixelBuffer)
let scaleX = ciImage.extent.width / CGFloat(maskW)
let scaleY = ciImage.extent.height / CGFloat(maskH)
let scaledMask = ciMask.transformed(by: CGAffineTransform(scaleX: scaleX, y: scaleY))
let context = CIContext()
guard let cgImage = context.createCGImage(scaledMask, from: ciImage.extent) else {
    fputs("failed to create CGImage\n", stderr)
    exit(1)
}
let rep = NSBitmapImageRep(cgImage: cgImage)
rep.size = NSSize(width: cgImage.width, height: cgImage.height)
guard let pngData = rep.representation(using: .png, properties: [:]) else {
    fputs("failed to create PNG data\n", stderr)
    exit(1)
}
do {
    try pngData.write(to: URL(fileURLWithPath: outputPath))
} catch {
    fputs("failed to write \(outputPath): \(error)\n", stderr)
    exit(1)
}
