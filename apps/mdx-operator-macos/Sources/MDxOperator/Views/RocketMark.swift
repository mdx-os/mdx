import SwiftUI

/// A small, brand-matched rocket glyph drawn as a filled vector path.
///
/// SF Symbols ships no rocket, and this SwiftPM app has no asset catalog, so we
/// draw the mark ourselves. It renders as a monochrome template that inherits
/// `.foregroundStyle`, so it tints accent/secondary exactly like the SF Symbols
/// beside it in the sidebar nav. The silhouette mirrors the app/dock icon rocket
/// (pointed nose cone, straight body, two swept fins, small exhaust) simplified
/// for legibility at ~14-16pt: no porthole, no ember gradient, which would only
/// muddy the mark at nav scale.
struct RocketMark: View {
  /// Overall glyph height in points, tuned to sit flush beside a
  /// `.font(.system(size: 14))` SF Symbol.
  var size: CGFloat = 15

  var body: some View {
    RocketShape()
      // Slim upright silhouette; width tracks the app-icon body proportions.
      .frame(width: size * 0.72, height: size)
  }
}

/// The rocket outline: body and both fins are one continuous closed path (so
/// there are no winding seams), with a small exhaust flame as a separate
/// non-overlapping subpath below the engine. Coordinates are fractions of the
/// draw rect (y-down), so the mark scales crisply at any point size.
struct RocketShape: Shape {
  func path(in rect: CGRect) -> Path {
    var p = Path()
    func pt(_ fx: CGFloat, _ fy: CGFloat) -> CGPoint {
      CGPoint(x: rect.minX + fx * rect.width, y: rect.minY + fy * rect.height)
    }

    let cx: CGFloat = 0.5
    let hw: CGFloat = 0.20   // body half-width
    let finW: CGFloat = 0.15 // fin reach beyond the body

    // Single outline: down the right (nose -> shoulder -> fin -> engine base),
    // across the base, then up the left as a mirror back to the nose.
    p.move(to: pt(cx, 0.03))
    // right ogive nose
    p.addQuadCurve(to: pt(cx + hw, 0.33), control: pt(cx + hw * 0.98, 0.10))
    // body right edge down to the fin leading edge
    p.addLine(to: pt(cx + hw, 0.52))
    // sweep out to the right fin tip and back in to the engine shoulder
    p.addLine(to: pt(cx + hw + finW, 0.80))
    p.addLine(to: pt(cx + hw * 0.82, 0.72))
    // rounded engine base to center
    p.addQuadCurve(to: pt(cx, 0.75), control: pt(cx + hw * 0.82, 0.75))
    // left mirror
    p.addQuadCurve(to: pt(cx - hw * 0.82, 0.72), control: pt(cx - hw * 0.82, 0.75))
    p.addLine(to: pt(cx - hw - finW, 0.80))
    p.addLine(to: pt(cx - hw, 0.52))
    p.addLine(to: pt(cx - hw, 0.33))
    p.addQuadCurve(to: pt(cx, 0.03), control: pt(cx - hw * 0.98, 0.10))
    p.closeSubpath()

    // Small exhaust flame tucked just under the engine (separate subpath).
    p.move(to: pt(cx - hw * 0.4, 0.77))
    p.addQuadCurve(to: pt(cx, 0.96), control: pt(cx - hw * 0.18, 0.86))
    p.addQuadCurve(to: pt(cx + hw * 0.4, 0.77), control: pt(cx + hw * 0.18, 0.86))
    p.closeSubpath()

    return p
  }
}
