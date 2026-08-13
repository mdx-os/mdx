// Generates the MDx app icon: a deep forge-gradient squircle with a white
// rocket mark and an ember exhaust accent, rendered at every macOS icon
// size into an .iconset (iconutil then packs the .icns).
//
// Run: swift script/make_icon.swift <output-iconset-dir>
import AppKit

let args = CommandLine.arguments
guard args.count >= 2 else {
  fputs("usage: swift make_icon.swift <output-iconset-dir>\n", stderr)
  exit(2)
}
let outDir = URL(fileURLWithPath: args[1])
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

func drawIcon(size: CGFloat) -> NSImage {
  let image = NSImage(size: NSSize(width: size, height: size))
  image.lockFocus()
  defer { image.unlockFocus() }

  guard let ctx = NSGraphicsContext.current?.cgContext else { return image }

  // macOS icon grid: the squircle occupies ~80% of the canvas, centered.
  let inset = size * 0.10
  let rect = CGRect(x: inset, y: inset, width: size - inset * 2, height: size - inset * 2)
  let radius = rect.width * 0.225
  let squircle = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)

  // Drop shadow (subtle, downward) so the icon sits on light and dark docks.
  ctx.saveGState()
  ctx.setShadow(offset: CGSize(width: 0, height: -size * 0.012), blur: size * 0.03,
                color: NSColor.black.withAlphaComponent(0.35).cgColor)
  ctx.addPath(squircle)
  ctx.setFillColor(NSColor.black.cgColor)
  ctx.fillPath()
  ctx.restoreGState()

  // Forge gradient: deep indigo night up to a warm ember horizon at the base.
  ctx.saveGState()
  ctx.addPath(squircle)
  ctx.clip()
  let colors = [
    NSColor(calibratedRed: 0.035, green: 0.035, blue: 0.055, alpha: 1).cgColor, // top: near-black, faint cool
    NSColor(calibratedRed: 0.060, green: 0.050, blue: 0.090, alpha: 1).cgColor, // mid: barely-lifted black
    NSColor(calibratedRed: 0.130, green: 0.075, blue: 0.105, alpha: 1).cgColor  // base: warm ember-black lift
  ] as CFArray
  let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors, locations: [0.0, 0.62, 1.0])!
  ctx.drawLinearGradient(gradient,
                         start: CGPoint(x: rect.midX, y: rect.maxY),
                         end: CGPoint(x: rect.midX, y: rect.minY),
                         options: [])

  // Ember glow pooled low behind the rocket exhaust.
  let glowColors = [
    NSColor(calibratedRed: 1.0, green: 0.62, blue: 0.32, alpha: 0.55).cgColor,
    NSColor(calibratedRed: 1.0, green: 0.62, blue: 0.32, alpha: 0.0).cgColor
  ] as CFArray
  let glow = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: glowColors, locations: [0, 1])!
  ctx.drawRadialGradient(glow,
                         startCenter: CGPoint(x: rect.midX, y: rect.minY + rect.height * 0.22), startRadius: 0,
                         endCenter: CGPoint(x: rect.midX, y: rect.minY + rect.height * 0.22), endRadius: rect.width * 0.55,
                         options: [])

  // Inner top edge highlight for depth.
  let edgeColors = [
    NSColor.white.withAlphaComponent(0.22).cgColor,
    NSColor.white.withAlphaComponent(0.0).cgColor
  ] as CFArray
  let edge = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: edgeColors, locations: [0, 1])!
  ctx.drawLinearGradient(edge,
                         start: CGPoint(x: rect.midX, y: rect.maxY),
                         end: CGPoint(x: rect.midX, y: rect.maxY - rect.height * 0.18),
                         options: [])
  ctx.restoreGState()

  // The rocket mark, drawn as vector paths so it stays a bold, legible
  // silhouette from 1024px down to a 16px dock tile. Coordinates are y-up.
  // A white nose-cone + body + swept fins ride above an ember exhaust flame
  // that reuses the icon's accent palette.
  let cx = rect.midX
  let cy = rect.midY + rect.height * 0.04            // nudge above center, like the old glyph
  let H = rect.height * 0.46                         // overall rocket height
  let hw = rect.width * 0.125                         // body half-width
  let topY = cy + H * 0.50                            // nose apex
  let shoulderY = cy + H * 0.10                       // where the nose meets the body
  let baseY = cy - H * 0.36                           // engine base

  // Ember exhaust flame: a teardrop below the engine, hottest near the body.
  let flameW = hw * 0.92
  let flameTopY = baseY + H * 0.03                    // tuck slightly under the base
  let flameTipY = baseY - H * 0.30
  let flame = CGMutablePath()
  flame.move(to: CGPoint(x: cx - flameW, y: flameTopY))
  flame.addQuadCurve(to: CGPoint(x: cx, y: flameTipY),
                     control: CGPoint(x: cx - flameW * 0.55, y: flameTipY + H * 0.02))
  flame.addQuadCurve(to: CGPoint(x: cx + flameW, y: flameTopY),
                     control: CGPoint(x: cx + flameW * 0.55, y: flameTipY + H * 0.02))
  flame.addQuadCurve(to: CGPoint(x: cx - flameW, y: flameTopY),
                     control: CGPoint(x: cx, y: flameTopY + H * 0.10))
  flame.closeSubpath()

  // Tilt the whole rocket (exhaust, body, fins, window) so it climbs to the
  // upper-right: reads as lift-off at an angle rather than a static upright mark.
  // Rotate about the rocket's own pivot so it stays centered in the squircle.
  ctx.saveGState()
  ctx.translateBy(x: cx, y: cy)
  ctx.rotate(by: -.pi / 4)   // 45 degrees clockwise: nose points up and to the right
  ctx.translateBy(x: -cx, y: -cy)

  // Warm glow pooled behind the flame for lift.
  ctx.saveGState()
  let flameGlowColors = [
    NSColor(calibratedRed: 1.0, green: 0.66, blue: 0.30, alpha: 0.55).cgColor,
    NSColor(calibratedRed: 1.0, green: 0.66, blue: 0.30, alpha: 0.0).cgColor
  ] as CFArray
  let flameGlow = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: flameGlowColors, locations: [0, 1])!
  ctx.drawRadialGradient(flameGlow,
                         startCenter: CGPoint(x: cx, y: baseY - H * 0.06), startRadius: 0,
                         endCenter: CGPoint(x: cx, y: baseY - H * 0.06), endRadius: hw * 3.0,
                         options: [])
  ctx.restoreGState()

  // Outer flame: yellow at the engine grading to orange at the tip.
  ctx.saveGState()
  ctx.addPath(flame)
  ctx.clip()
  let flameColors = [
    NSColor(calibratedRed: 1.0, green: 0.90, blue: 0.55, alpha: 1).cgColor, // near engine
    NSColor(calibratedRed: 1.0, green: 0.72, blue: 0.28, alpha: 1).cgColor,
    NSColor(calibratedRed: 0.98, green: 0.45, blue: 0.20, alpha: 1).cgColor  // tip
  ] as CFArray
  let flameGrad = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: flameColors, locations: [0, 0.5, 1])!
  ctx.drawLinearGradient(flameGrad,
                         start: CGPoint(x: cx, y: flameTopY),
                         end: CGPoint(x: cx, y: flameTipY),
                         options: [])
  ctx.restoreGState()

  // Bright inner flame core for depth.
  let core = CGMutablePath()
  let coreW = flameW * 0.5
  core.move(to: CGPoint(x: cx - coreW, y: flameTopY))
  core.addQuadCurve(to: CGPoint(x: cx, y: flameTipY + H * 0.10),
                    control: CGPoint(x: cx - coreW * 0.6, y: flameTipY + H * 0.10))
  core.addQuadCurve(to: CGPoint(x: cx + coreW, y: flameTopY),
                    control: CGPoint(x: cx + coreW * 0.6, y: flameTipY + H * 0.10))
  core.addQuadCurve(to: CGPoint(x: cx - coreW, y: flameTopY),
                    control: CGPoint(x: cx, y: flameTopY + H * 0.06))
  core.closeSubpath()
  ctx.saveGState()
  ctx.addPath(core)
  ctx.setFillColor(NSColor(calibratedRed: 1.0, green: 0.96, blue: 0.80, alpha: 0.95).cgColor)
  ctx.fillPath()
  ctx.restoreGState()

  // White mark = body + two swept fins. Each part is filled separately into an
  // offscreen layer so overlaps union cleanly (no winding seams), then the whole
  // mark is drawn once with a soft drop shadow.
  let finW = rect.width * 0.10
  let finTopY = cy + H * 0.00
  let finOuterY = baseY - H * 0.05

  // Body: pointed ogive nose, straight sides, softly rounded engine base.
  let bodyPath = CGMutablePath()
  bodyPath.move(to: CGPoint(x: cx, y: topY))
  bodyPath.addQuadCurve(to: CGPoint(x: cx + hw, y: shoulderY),
                        control: CGPoint(x: cx + hw * 0.98, y: topY - H * 0.22))
  bodyPath.addLine(to: CGPoint(x: cx + hw, y: baseY + H * 0.05))
  bodyPath.addQuadCurve(to: CGPoint(x: cx, y: baseY),
                        control: CGPoint(x: cx + hw, y: baseY))
  bodyPath.addQuadCurve(to: CGPoint(x: cx - hw, y: baseY + H * 0.05),
                        control: CGPoint(x: cx - hw, y: baseY))
  bodyPath.addLine(to: CGPoint(x: cx - hw, y: shoulderY))
  bodyPath.addQuadCurve(to: CGPoint(x: cx, y: topY),
                        control: CGPoint(x: cx - hw * 0.98, y: topY - H * 0.22))
  bodyPath.closeSubpath()

  let rightFin = CGMutablePath()
  rightFin.move(to: CGPoint(x: cx + hw * 0.85, y: finTopY))
  rightFin.addQuadCurve(to: CGPoint(x: cx + hw + finW, y: finOuterY),
                        control: CGPoint(x: cx + hw + finW * 0.55, y: finTopY - H * 0.06))
  rightFin.addLine(to: CGPoint(x: cx + hw * 0.55, y: baseY))
  rightFin.closeSubpath()

  let leftFin = CGMutablePath()
  leftFin.move(to: CGPoint(x: cx - hw * 0.85, y: finTopY))
  leftFin.addQuadCurve(to: CGPoint(x: cx - hw - finW, y: finOuterY),
                       control: CGPoint(x: cx - hw - finW * 0.55, y: finTopY - H * 0.06))
  leftFin.addLine(to: CGPoint(x: cx - hw * 0.55, y: baseY))
  leftFin.closeSubpath()

  let mark = NSImage(size: NSSize(width: size, height: size))
  mark.lockFocus()
  if let markCtx = NSGraphicsContext.current?.cgContext {
    markCtx.setFillColor(NSColor.white.cgColor)
    for part in [bodyPath, rightFin, leftFin] {
      markCtx.addPath(part)
      markCtx.fillPath()
    }
  }
  mark.unlockFocus()

  // Draw the unioned white mark once, with a soft downward shadow so it lifts
  // off the gradient.
  ctx.saveGState()
  ctx.setShadow(offset: CGSize(width: 0, height: -size * 0.008), blur: size * 0.02,
                color: NSColor.black.withAlphaComponent(0.45).cgColor)
  mark.draw(at: .zero, from: .zero, operation: .sourceOver, fraction: 1.0)
  ctx.restoreGState()

  // Porthole window: a deep-indigo glass disc punched into the upper body.
  let windowR = hw * 0.52
  let windowY = cy + H * 0.16
  ctx.saveGState()
  ctx.addEllipse(in: CGRect(x: cx - windowR, y: windowY - windowR,
                            width: windowR * 2, height: windowR * 2))
  ctx.setFillColor(NSColor(calibratedRed: 0.16, green: 0.17, blue: 0.36, alpha: 1).cgColor)
  ctx.fillPath()
  // Tiny top highlight on the glass.
  let glintR = windowR * 0.42
  ctx.addEllipse(in: CGRect(x: cx - glintR * 1.1, y: windowY + windowR * 0.05,
                            width: glintR, height: glintR))
  ctx.setFillColor(NSColor.white.withAlphaComponent(0.30).cgColor)
  ctx.fillPath()
  ctx.restoreGState()

  // Close the rocket tilt transform opened before the exhaust.
  ctx.restoreGState()

  return image
}

func writePNG(_ image: NSImage, pixels: Int, to url: URL) {
  let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                             bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                             colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  rep.size = NSSize(width: pixels, height: pixels)
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
  image.draw(in: NSRect(x: 0, y: 0, width: pixels, height: pixels),
             from: .zero, operation: .copy, fraction: 1.0)
  NSGraphicsContext.restoreGraphicsState()
  try? rep.representation(using: .png, properties: [:])?.write(to: url)
}

// Standard macOS iconset entries.
let entries: [(String, Int)] = [
  ("icon_16x16.png", 16), ("icon_16x16@2x.png", 32),
  ("icon_32x32.png", 32), ("icon_32x32@2x.png", 64),
  ("icon_128x128.png", 128), ("icon_128x128@2x.png", 256),
  ("icon_256x256.png", 256), ("icon_256x256@2x.png", 512),
  ("icon_512x512.png", 512), ("icon_512x512@2x.png", 1024),
]

for (name, pixels) in entries {
  let image = drawIcon(size: CGFloat(pixels))
  writePNG(image, pixels: pixels, to: outDir.appendingPathComponent(name))
}
print("iconset written to \(outDir.path)")
