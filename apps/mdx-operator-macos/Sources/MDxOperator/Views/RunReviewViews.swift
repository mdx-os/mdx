import AppKit
import SwiftUI

struct RunRecoveryBanner: View {
  let surface: RunRecoverySurface
  let reviewProof: () -> Void
  var openDiff: (() -> Void)?
  var steer: (() -> Void)?
  var pickBackUp: (() -> Void)?
  var startSmaller: (() -> Void)?

  init(
    run: ForgeRun,
    reviewProof: @escaping () -> Void,
    openDiff: (() -> Void)? = nil,
    steer: (() -> Void)? = nil,
    pickBackUp: (() -> Void)? = nil,
    startSmaller: (() -> Void)? = nil
  ) {
    self.init(
      surface: RunRecoverySurface(run: run),
      reviewProof: reviewProof,
      openDiff: openDiff,
      steer: steer,
      pickBackUp: pickBackUp,
      startSmaller: startSmaller
    )
  }

  init(
    surface: RunRecoverySurface,
    reviewProof: @escaping () -> Void,
    openDiff: (() -> Void)? = nil,
    steer: (() -> Void)? = nil,
    pickBackUp: (() -> Void)? = nil,
    startSmaller: (() -> Void)? = nil
  ) {
    self.surface = surface
    self.reviewProof = reviewProof
    self.openDiff = openDiff
    self.steer = steer
    self.pickBackUp = pickBackUp
    self.startSmaller = startSmaller
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(alignment: .firstTextBaseline, spacing: 10) {
        Label(
          surface.title,
          systemImage: surface.isProofCaveat
            ? "checkmark.seal"
            : (surface.statusPillLabel == "Still working" ? "wrench.and.screwdriver" : "exclamationmark.triangle")
        )
          .font(.callout.weight(.semibold))
        Spacer(minLength: 8)
        if surface.statusPillLabel == "Still working" {
          StatusPill(status: surface.statusPillLabel, tone: .neutral)
        } else {
          StatusPill(status: surface.statusPillLabel, tone: surface.isProofCaveat ? .positive : .locked)
        }
      }

      if !surface.branchIdentity.isEmpty {
        // The raw branch lives in the header meta (copyable); the banner just
        // points at it.
        Text("on the run branch")
          .font(.caption.weight(.medium))
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      Text(surface.recoveryLine)
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)

      HStack(spacing: 8) {
        Button {
          reviewProof()
        } label: {
          Label("Review proof", systemImage: "checkmark.seal")
        }
        .controlSize(.small)

        if let openDiff {
          Button {
            openDiff()
          } label: {
            Label("Open diff", systemImage: "doc.text.magnifyingglass")
          }
          .controlSize(.small)
        }

        if let steer {
          Button {
            steer()
          } label: {
            Label("Steer", systemImage: "location.north.line")
          }
          .controlSize(.small)
        }

        if let pickBackUp {
          Button {
            pickBackUp()
          } label: {
            Label(
              surface.revisionControlLabel,
              systemImage: surface.revisionControlSymbol
            )
          }
          .controlSize(.small)
        } else if let startSmaller {
          Button {
            startSmaller()
          } label: {
            Label(
              surface.isBaselineRed ? "Repair baseline" : "Start smaller",
              systemImage: surface.isBaselineRed ? "wrench.adjustable" : "arrow.down.right.and.arrow.up.left"
            )
          }
          .controlSize(.small)
        }

        Spacer(minLength: 0)
      }
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill((surface.isProofCaveat ? Color.green : Color.orange).opacity(surface.statusPillLabel == "Still working" ? 0.07 : 0.10))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .stroke((surface.isProofCaveat ? Color.green : Color.orange).opacity(0.18), lineWidth: 1)
    )
  }
}

struct ReviewPacketView: View {
  let packet: ReviewPacket
  let proofRecovered: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      if !packet.nextMove.isEmpty {
        Label(packet.nextMove, systemImage: "arrow.forward.circle")
          .font(.callout.weight(.medium))
          .padding(12)
          .frame(maxWidth: .infinity, alignment: .leading)
          .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
              .fill(Color.accentColor.opacity(0.08))
          )
      }

      HStack(spacing: 10) {
        if !packet.reviewStatus.isEmpty {
          StatusPill(status: humanize(packet.reviewStatus), tone: packet.reviewStatus.contains("ready") ? .positive : .neutral)
        }
        StatusPill(status: "\(packet.checksPassed) passed", tone: packet.checksPassed > 0 ? .positive : .neutral)
        if packet.checksFailed > 0 {
          StatusPill(
            status: proofRecovered ? "\(packet.checksFailed) earlier failures" : "\(packet.checksFailed) failed",
            tone: proofRecovered ? .neutral : .locked
          )
        }
        if packet.shipDecided {
          StatusPill(status: "Shipped", tone: .positive)
        }
      }

      if !decisionLines.isEmpty {
        packetGroup(title: "Decision brief", lines: decisionLines, mono: false)
      }
      if !proofLines.isEmpty {
        packetGroup(title: "Proof that ran", lines: proofLines, mono: false)
      }
      if !checklistLines.isEmpty {
        packetGroup(title: "Review checklist", lines: checklistLines, mono: false)
      }
      if !packet.missingChecks.isEmpty {
        packetGroup(title: "Still missing", lines: packet.missingChecks.map(readableEvidenceLine), mono: false, warn: true)
      }
      if !rawEvidenceLines.isEmpty {
        DisclosureGroup {
          packetGroup(title: "Raw evidence", lines: rawEvidenceLines, mono: true)
            .padding(.top, 6)
        } label: {
          Label("\(rawEvidenceLines.count) raw evidence line\(rawEvidenceLines.count == 1 ? "" : "s")", systemImage: "doc.text.magnifyingglass")
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
        }
      }
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }

  private var decisionLines: [String] {
    var lines: [String] = []
    if !packet.behaviorSummary.isEmpty {
      lines.append(readableEvidenceLine(packet.behaviorSummary))
    }
    if packet.shipDecided, !packet.shipReason.isEmpty {
      lines.append("Ship decision recorded: \(packet.shipReason)")
    }
    return lines
  }

  private var proofLines: [String] {
    var lines: [String] = []
    if packet.checksPassed > 0 || packet.checksFailed > 0 {
      if proofRecovered && packet.checksFailed > 0 {
        lines.append("The selected proof now passes. \(packet.checksFailed) earlier failed attempt\(packet.checksFailed == 1 ? "" : "s") remain in history.")
      } else {
        let failed = packet.checksFailed > 0 ? ", \(packet.checksFailed) failed" : ""
        lines.append("\(packet.checksPassed) check\(packet.checksPassed == 1 ? "" : "s") passed\(failed).")
      }
    }
    lines.append(contentsOf: packet.checkNames.map(readableEvidenceLine))
    lines.append(contentsOf: packet.satisfiedChecks.map(readableEvidenceLine).filter { !looksRaw($0) })
    return Array(collapseDuplicateProofCommands(lines).prefix(6))
  }

  private var checklistLines: [String] {
    let readable = packet.principalChecklist
      .map(readableEvidenceLine)
      .filter { !$0.isEmpty && !looksRaw($0) }
    return Array(collapseDuplicateProofCommands(readable).prefix(6))
  }

  private var rawEvidenceLines: [String] {
    let raw = packet.principalChecklist + packet.satisfiedChecks + packet.handoffLines
    return orderedUnique(raw.filter(looksRaw))
  }

  private func packetGroup(title: String, lines: [String], mono: Bool, warn: Bool = false) -> some View {
    VStack(alignment: .leading, spacing: 5) {
      Text(title)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
      ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
        HStack(alignment: .top, spacing: 6) {
          Image(systemName: warn ? "exclamationmark.circle" : "checkmark.circle")
            .font(.caption2)
            .foregroundStyle(warn ? Color.orange : Color.green)
          Text(line)
            .font(mono ? .caption.monospaced() : .callout)
            .foregroundStyle(.primary)
            .fixedSize(horizontal: false, vertical: true)
        }
      }
    }
  }

  private func orderedUnique(_ lines: [String]) -> [String] {
    var seen = Set<String>()
    var result: [String] = []
    for line in lines {
      let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmed.isEmpty, seen.insert(trimmed).inserted else { continue }
      result.append(trimmed)
    }
    return result
  }

  private func collapseDuplicateProofCommands(_ lines: [String]) -> [String] {
    let unique = orderedUnique(lines)
    let namedCommands = Set(unique.compactMap(namedProofCommand))
    guard !namedCommands.isEmpty else { return unique }
    return unique.filter { line in
      !namedCommands.contains(line.trimmingCharacters(in: .whitespacesAndNewlines))
    }
  }

  private func namedProofCommand(_ line: String) -> String? {
    for prefix in ["Baseline proof: ", "Selected proof: "] {
      if line.hasPrefix(prefix) {
        return String(line.dropFirst(prefix.count)).trimmingCharacters(in: .whitespacesAndNewlines)
      }
    }
    return nil
  }

  private func readableEvidenceLine(_ raw: String) -> String {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    if let command = makeCommand(in: trimmed) {
      if trimmed.contains("baseline_run_command") {
        return "Baseline proof: \(command)"
      }
      if trimmed.contains("run_command") || trimmed.contains("exact_selected_command") || trimmed.contains("observed_check") {
        return "Selected proof: \(command)"
      }
      return command
    }
    if let command = value(after: "name = \"", in: trimmed) ?? value(after: "name=\"", in: trimmed) {
      return command
    }
    return trimmed
      .replacingOccurrences(of: "_", with: " ")
      .replacingOccurrences(of: "  ", with: " ")
  }

  private func makeCommand(in text: String) -> String? {
    guard let range = text.range(of: "make ") else { return nil }
    let suffix = text[range.lowerBound...]
    let stops = [" exit=", " tail=", ";", ")", ",", "\""]
    let end = stops
      .compactMap { suffix.range(of: $0)?.lowerBound }
      .min() ?? suffix.endIndex
    let command = suffix[..<end].trimmingCharacters(in: .whitespacesAndNewlines)
    return command.isEmpty ? nil : command
  }

  private func value(after marker: String, in text: String) -> String? {
    guard let range = text.range(of: marker) else { return nil }
    let suffix = text[range.upperBound...]
    let end = suffix.firstIndex { $0 == "\"" || $0 == ";" || $0 == ")" || $0 == "," } ?? suffix.endIndex
    let value = suffix[..<end].trimmingCharacters(in: .whitespacesAndNewlines)
    return value.isEmpty ? nil : value
  }

  private func looksRaw(_ line: String) -> Bool {
    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.hasPrefix("{")
      || trimmed.hasPrefix("[")
      || trimmed.contains(" = {")
      || trimmed.contains("\"")
      || trimmed.contains("observed_checks")
      || trimmed.contains("generated_from")
      || trimmed.contains("proof_commands")
  }

  private func humanize(_ raw: String) -> String {
    raw.replacingOccurrences(of: "_", with: " ").capitalized
  }
}
