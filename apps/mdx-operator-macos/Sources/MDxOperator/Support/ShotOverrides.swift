import SwiftUI

/// Deterministic capture overrides for the visual-QA rail.
///
/// Every value is derived from a `MDX_SHOT_*` environment variable and is inert
/// unless that variable is set, so the shipping app is undisturbed. The gallery
/// capture script (`script/capture_gallery.sh`) sets these to drive the
/// light/dark and accessibility matrix WITHOUT mutating the reviewer's System
/// Settings: the app forces the color scheme and accessibility environment in
/// process, then the script window-captures the result.
///
/// Honesty note on the accessibility matrix. A still-frame rail can only
/// meaningfully capture settings that change the pixels of a static frame:
///
/// - Larger Dynamic Type changes layout (wrapping, truncation, overflow), so it
///   IS forced, via `.dynamicTypeSize`, and captured.
/// - Reduce Motion only changes transitions and continuous animation, which do
///   not appear in a still frame. The app already gates every animation on
///   `accessibilityReduceMotion` (grep-verifiable), and that key is read-only so
///   it cannot be forced per process anyway. The rail documents it rather than
///   capturing frames identical to the default.
/// - Increase Contrast has no supported per-process override (`colorSchemeContrast`
///   is read-only, derived from the system setting), so it stays a manual,
///   system-level check. The rail does not fake it.
enum ShotOverrides {
  /// MDX_SHOT_APPEARANCE=light|dark forces the window color scheme, overriding
  /// the operator's saved appearance. Unset or unknown leaves it alone.
  static var colorScheme: ColorScheme? {
    colorScheme(for: ProcessInfo.processInfo.environment["MDX_SHOT_APPEARANCE"])
  }

  static func colorScheme(for raw: String?) -> ColorScheme? {
    switch raw?.lowercased() {
    case "light": return .light
    case "dark": return .dark
    default: return nil
    }
  }

  /// MDX_SHOT_A11Y selects one accessibility setting to force for the capture.
  /// Only settings that change a static frame belong here (see the type note);
  /// today that is larger Dynamic Type.
  enum Accessibility: String, CaseIterable {
    case largeText = "large_text"
  }

  static var accessibility: Accessibility? {
    accessibility(for: ProcessInfo.processInfo.environment["MDX_SHOT_A11Y"])
  }

  static func accessibility(for raw: String?) -> Accessibility? {
    guard let raw = raw?.lowercased(), !raw.isEmpty else { return nil }
    return Accessibility(rawValue: raw)
  }

  /// Stable slug for an appearance + a11y combination, used to name capture
  /// files deterministically (e.g. "dark", "light", "dark-reduce_motion"). Kept
  /// in the app (not just the script) so the naming contract is pinned by the
  /// mapper checks and cannot drift from what the script writes.
  static func variantSlug(appearance: String?, a11y: String?) -> String {
    let scheme = colorScheme(for: appearance) == .light ? "light" : "dark"
    if let a = accessibility(for: a11y) { return "\(scheme)-\(a.rawValue)" }
    return scheme
  }

  /// Selects a named page for the real-window gallery. Underscores make the
  /// value safe to carry through the TSV launch contract without shell quoting.
  static func pageTitle(
    in environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String? {
    guard let raw = environment["MDX_SHOT_PAGE_TITLE"]?
      .trimmingCharacters(in: .whitespacesAndNewlines),
      !raw.isEmpty
    else { return nil }
    return raw.replacingOccurrences(of: "_", with: " ")
  }
}

extension View {
  /// Apply the active capture accessibility override. Inert when no capture var
  /// is set, so it is safe to leave on the shipping view tree.
  func shotAccessibilityOverrides() -> some View {
    modifier(ShotAccessibilityModifier())
  }
}

private struct ShotAccessibilityModifier: ViewModifier {
  func body(content: Content) -> some View {
    switch ShotOverrides.accessibility {
    case .largeText:
      content.dynamicTypeSize(.accessibility2)
    case nil:
      content
    }
  }
}
