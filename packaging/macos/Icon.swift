import AppKit

// Render the existing docs/logo.svg geometry as a high-resolution macOS icon.
let side = 1024
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
let transform = NSAffineTransform()
transform.translateX(by: 0, yBy: CGFloat(side))
transform.scaleX(by: CGFloat(side) / 64, yBy: -CGFloat(side) / 64)
transform.concat()
NSColor(red: 23/255, green: 33/255, blue: 43/255, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: 64, height: 64), xRadius: 14, yRadius: 14).fill()
NSColor(red: 50/255, green: 199/255, blue: 147/255, alpha: 1).setStroke()
let leaf = NSBezierPath()
leaf.lineWidth = 3.2
leaf.lineCapStyle = .round
leaf.lineJoinStyle = .round
leaf.move(to: NSPoint(x: 30, y: 50))
leaf.curve(to: NSPoint(x: 27.6, y: 22.2), controlPoint1: NSPoint(x: 11, y: 50), controlPoint2: NSPoint(x: 12, y: 25))
leaf.curve(to: NSPoint(x: 46, y: 14), controlPoint1: NSPoint(x: 39, y: 20), controlPoint2: NSPoint(x: 42, y: 19))
leaf.curve(to: NSPoint(x: 50, y: 30), controlPoint1: NSPoint(x: 48, y: 18), controlPoint2: NSPoint(x: 50, y: 22.4))
leaf.curve(to: NSPoint(x: 30, y: 50), controlPoint1: NSPoint(x: 50, y: 41), controlPoint2: NSPoint(x: 40.4, y: 50))
leaf.stroke()
let stem = NSBezierPath()
stem.lineWidth = 3.2
stem.lineCapStyle = .round
stem.move(to: NSPoint(x: 12, y: 52))
stem.curve(to: NSPoint(x: 22.2, y: 40), controlPoint1: NSPoint(x: 12, y: 46), controlPoint2: NSPoint(x: 15.7, y: 41.3))
stem.curve(to: NSPoint(x: 34, y: 34), controlPoint1: NSPoint(x: 27, y: 39.1), controlPoint2: NSPoint(x: 32, y: 36))
stem.stroke()
NSGraphicsContext.restoreGraphicsState()
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
