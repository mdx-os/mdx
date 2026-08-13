import SwiftUI

struct MemoryView: View {
  @Environment(OperatorStore.self) private var store
  @State private var showRail = true
  @State private var lessonToRetire: MemoryLesson?

  var body: some View {
    HStack(spacing: 0) {
      if showRail {
        memoryRail
          .frame(width: SurfaceMetrics.railWidth)
          .transition(.move(edge: .leading).combined(with: .opacity))
        Divider()
      }
      memoryDetail
    }
    .task { await store.loadMemory() }
    .sheet(item: $lessonToRetire) { lesson in
      RetireMemorySheet(lesson: lesson)
        .environment(store)
    }
  }

  private var memoryRail: some View {
    VStack(spacing: 0) {
      RailHeader(title: "Memory", count: store.memoryWorkspace.map(totalCount) ?? 0)
      SurfaceSearchField(placeholder: "Search memory", text: Bindable(store).memorySearch)
        .padding(.horizontal, 10)
        .padding(.bottom, 8)
      Divider()

      List(MemoryRail.allCases, selection: Bindable(store).selectedMemoryRail) { rail in
        MemoryRailRow(
          rail: rail,
          count: store.memoryWorkspace?.count(for: rail) ?? 0,
          selected: store.selectedMemoryRail == rail
        )
        .tag(rail)
      }
      .listStyle(.sidebar)

      VStack(alignment: .leading, spacing: 5) {
        Label("Three sources, kept distinct", systemImage: "checkmark.shield")
          .font(.caption.weight(.semibold))
        Text("Lessons, team memory, and model evidence never become one opaque brain.")
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      .padding(14)
    }
    .background(Color.primary.opacity(0.025))
  }

  private var memoryDetail: some View {
    VStack(spacing: 0) {
      SurfaceTopBar(
        icon: "brain.head.profile",
        title: "Memory",
        subtitle: "what MDx can responsibly recall",
        showRail: $showRail
      ) {
        Button {
          store.select(.twin)
        } label: {
          Label("Ask Twin", systemImage: "sparkles")
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .help("Use this context in a conversation with Twin")

        Button {
          Task { await store.loadMemory() }
        } label: {
          Image(systemName: "arrow.clockwise")
        }
        .buttonStyle(.plain)
        .help("Refresh Memory")
      }
      Divider()

      if let result = store.memoryActionResult {
        MemoryActionBanner(result: result)
        Divider()
      }

      if let workspace = store.memoryWorkspace {
        ScrollView {
          VStack(alignment: .leading, spacing: 20) {
            railHeading(workspace)
            railContent(workspace)
          }
          .frame(maxWidth: SurfaceMetrics.readingWidth, alignment: .leading)
          .padding(28)
          .frame(maxWidth: .infinity, alignment: .top)
        }
      } else {
        SurfaceDetailPlaceholder(
          connected: store.snapshot.connectionStatus == .ok,
          phase: store.memoryPhase,
          isEmpty: true,
          icon: "brain.head.profile",
          selectPrompt: "Choose what to review",
          emptyTitle: "Nothing has been kept yet",
          emptyMessage: "MDx will show approved lessons, team memory, and model evidence here as they earn a place.",
          retry: { Task { await store.loadMemory() } }
        )
      }
    }
  }

  @ViewBuilder private func railHeading(_ workspace: MemoryWorkspace) -> some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack(alignment: .firstTextBaseline) {
        Text(store.selectedMemoryRail.title)
          .font(SurfaceType.title)
        Spacer()
        StatusPill(status: "\(workspace.count(for: store.selectedMemoryRail))")
      }
      Text(store.selectedMemoryRail.subtitle)
        .font(SurfaceType.callout)
        .foregroundStyle(.secondary)
      if case .stale(let message) = store.memoryPhase {
        Label(message, systemImage: "clock.arrow.circlepath")
          .font(.caption)
          .foregroundStyle(.orange)
      }
    }
  }

  @ViewBuilder private func railContent(_ workspace: MemoryWorkspace) -> some View {
    if workspace.unavailableRails.contains(store.selectedMemoryRail) {
      MemoryEmptyRail(
        icon: "exclamationmark.triangle",
        title: "This source is unavailable",
        message: "The other Memory rails can still be reviewed. Refresh when the local projection is reachable again."
      )
    } else {
      switch store.selectedMemoryRail {
    case .learned:
      let lessons = filteredLessons(workspace.lessons)
      if lessons.isEmpty {
        MemoryEmptyRail(
          icon: "checkmark.seal",
          title: store.memorySearch.isEmpty ? "No approved lessons yet" : "No lessons match your search",
          message: "Lessons only appear after a person promotes and activates them."
        )
      } else {
        ForEach(lessons) { lesson in
          MemoryLessonCard(lesson: lesson) { lessonToRetire = lesson }
        }
        if workspace.retiredLessonCount > 0 {
          Text("\(workspace.retiredLessonCount) outdated \(workspace.retiredLessonCount == 1 ? "lesson remains" : "lessons remain") in the receipt history and no longer guides work.")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
    case .recorded:
      let groups = filteredGroups(workspace.surfaceGroups)
      if groups.isEmpty {
        MemoryEmptyRail(
          icon: "tray.full",
          title: store.memorySearch.isEmpty ? "No team memory yet" : "No team memory matches your search",
          message: "MDx records proposals here first. They become durable team memory only after review."
        )
      } else {
        ForEach(groups) { group in
          MemorySurfaceSection(group: group)
        }
      }
    case .modelProof:
      let proof = filteredModelProof(workspace.modelProof)
      if proof.isEmpty {
        MemoryEmptyRail(
          icon: "chart.bar.xaxis",
          title: store.memorySearch.isEmpty ? "No model evidence yet" : "No evidence matches your search",
          message: "Completed Forge runs will build an evidence trail by model and kind of work."
        )
      } else {
        Text("This is a scorecard from completed runs. It is evidence, not a lesson MDx has learned.")
          .font(.callout)
          .foregroundStyle(.secondary)
          .padding(.bottom, 2)
        VStack(spacing: 0) {
          ForEach(Array(proof.enumerated()), id: \.element.id) { index, item in
            MemoryModelProofRow(item: item)
            if index < proof.count - 1 { Divider().padding(.leading, 16) }
          }
        }
        .background(.quaternary.opacity(0.25), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
          RoundedRectangle(cornerRadius: 12, style: .continuous)
            .stroke(Color.primary.opacity(0.08))
        }
      }
      }
    }
  }

  private func totalCount(_ workspace: MemoryWorkspace) -> Int {
    MemoryRail.allCases.reduce(0) { $0 + workspace.count(for: $1) }
  }

  private var query: String {
    store.memorySearch.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private func filteredLessons(_ lessons: [MemoryLesson]) -> [MemoryLesson] {
    guard !query.isEmpty else { return lessons }
    return lessons.filter {
      [$0.summary, $0.targetType, $0.reviewOwner, $0.activationBasis]
        .joined(separator: " ").localizedCaseInsensitiveContains(query)
    }
  }

  private func filteredGroups(_ groups: [MemorySurfaceGroup]) -> [MemorySurfaceGroup] {
    guard !query.isEmpty else { return groups }
    return groups.compactMap { group in
      let matches = group.records.filter {
        [group.surface, $0.content, $0.note, $0.proposedBy, $0.reviewedBy]
          .joined(separator: " ").localizedCaseInsensitiveContains(query)
      }
      return matches.isEmpty ? nil : MemorySurfaceGroup(surface: group.surface, records: matches)
    }
  }

  private func filteredModelProof(_ proof: [MemoryModelProof]) -> [MemoryModelProof] {
    guard !query.isEmpty else { return proof }
    return proof.filter {
      [$0.model, $0.workType].joined(separator: " ").localizedCaseInsensitiveContains(query)
    }
  }
}

private struct MemoryRailRow: View {
  let rail: MemoryRail
  let count: Int
  let selected: Bool

  var body: some View {
    HStack(spacing: 10) {
      Image(systemName: rail.systemImage)
        .foregroundStyle(selected ? Color.accentColor : .secondary)
        .frame(width: 18)
      VStack(alignment: .leading, spacing: 2) {
        Text(rail.title).font(.subheadline.weight(.medium))
        Text(rail.subtitle).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
      }
      Spacer(minLength: 6)
      Text("\(count)").font(.caption.monospacedDigit()).foregroundStyle(.tertiary)
    }
    .padding(.vertical, 5)
  }
}

private struct MemoryLessonCard: View {
  let lesson: MemoryLesson
  let retire: () -> Void
  @State private var showsEvidence = false

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: "checkmark.seal.fill")
          .foregroundStyle(.green)
          .font(.title3)
        VStack(alignment: .leading, spacing: 6) {
          Text(lesson.summary).font(.body.weight(.medium)).textSelection(.enabled)
          HStack(spacing: 7) {
            StatusPill(status: humanize(lesson.targetType), tone: .neutral)
            if lesson.steersCasting {
              StatusPill(status: "Steers who plans this work", color: .blue)
            }
          }
        }
        Spacer(minLength: 16)
        Button("That’s outdated", action: retire)
          .buttonStyle(.bordered)
          .controlSize(.small)
          .help("Stop this lesson from guiding future work while preserving its history")
      }

      if !lesson.activationBasis.isEmpty {
        Text(lesson.activationBasis).font(.callout).foregroundStyle(.secondary)
      }

      DisclosureGroup("Why MDx may use this", isExpanded: $showsEvidence) {
        VStack(alignment: .leading, spacing: 8) {
          if !lesson.reviewOwner.isEmpty {
            LabeledContent("Review owner", value: lesson.reviewOwner)
          }
          if !lesson.rollbackPlan.isEmpty {
            LabeledContent("If it stops being true", value: lesson.rollbackPlan)
          }
          if !lesson.promotionReceiptID.isEmpty {
            LabeledContent("Promotion receipt", value: lesson.promotionReceiptID)
          }
          if !lesson.judgmentReceiptID.isEmpty {
            LabeledContent("Judgment receipt", value: lesson.judgmentReceiptID)
          }
        }
        .font(.caption)
        .textSelection(.enabled)
        .padding(.top, 8)
      }
      .font(.caption.weight(.medium))
    }
    .padding(18)
    .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.primary.opacity(0.08))
    }
  }

  private func humanize(_ value: String) -> String {
    value.replacingOccurrences(of: "_", with: " ").capitalized
  }
}

private struct MemorySurfaceSection: View {
  let group: MemorySurfaceGroup

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Text(group.surface).font(.headline)
        Spacer()
        let pending = group.records.filter { $0.kind == .pending }.count
        if pending > 0 { StatusPill(status: "\(pending) waiting for review", color: .orange) }
      }
      ForEach(group.records) { record in
        HStack(alignment: .top, spacing: 12) {
          Image(systemName: record.kind == .pending ? "clock.badge.questionmark" : "checkmark.circle.fill")
            .foregroundStyle(record.kind == .pending ? .orange : .green)
            .frame(width: 18)
          VStack(alignment: .leading, spacing: 6) {
            Text(record.content).font(.body).textSelection(.enabled)
            Text(record.kind == .pending
              ? "Proposed by \(record.proposedBy). This is not durable team memory until a teammate clears it."
              : "Kept after review by \(record.reviewedBy).")
              .font(.caption)
              .foregroundStyle(.secondary)
            let receipt = record.kind == .pending ? record.sourceReceiptID : record.decisionReceiptID
            if !receipt.isEmpty {
              Text("Receipt \(receipt)")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .textSelection(.enabled)
            }
          }
          Spacer(minLength: 0)
        }
        .padding(16)
        .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
      }
    }
  }
}

private struct MemoryModelProofRow: View {
  let item: MemoryModelProof

  var body: some View {
    HStack(spacing: 16) {
      VStack(alignment: .leading, spacing: 4) {
        Text(item.model).font(.body.weight(.medium))
        Text(item.workType.replacingOccurrences(of: "_", with: " ").capitalized)
          .font(.caption).foregroundStyle(.secondary)
      }
      Spacer()
      if item.runs < 5 { StatusPill(status: "Early signal", color: .orange) }
      metric("Runs", "\(item.runs)")
      metric("Done", "\(Int((item.doneRate * 100).rounded()))%")
      metric("Avg turns", String(format: "%.1f", item.averageTurns))
    }
    .padding(16)
  }

  private func metric(_ label: String, _ value: String) -> some View {
    VStack(alignment: .trailing, spacing: 2) {
      Text(value).font(.body.weight(.semibold).monospacedDigit())
      Text(label).font(.caption2).foregroundStyle(.tertiary)
    }
    .frame(minWidth: 58, alignment: .trailing)
  }
}

private struct MemoryActionBanner: View {
  let result: RunActionOutcome

  var body: some View {
    HStack(spacing: 10) {
      Image(systemName: result.succeeded ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
        .foregroundStyle(result.succeeded ? .green : .orange)
      VStack(alignment: .leading, spacing: 2) {
        Text(result.succeeded ? "Lesson retired" : "Memory was not changed").font(.subheadline.weight(.semibold))
        Text(result.succeeded
          ? "It stays in the receipt history and no longer guides future work."
          : result.detail)
          .font(.caption).foregroundStyle(.secondary)
      }
      Spacer()
      if !result.receiptID.isEmpty {
        Text(result.receiptID).font(.caption2.monospaced()).foregroundStyle(.tertiary).textSelection(.enabled)
      }
    }
    .padding(.horizontal, 20)
    .padding(.vertical, 10)
    .background((result.succeeded ? Color.green : Color.orange).opacity(0.07))
  }
}

private struct MemoryEmptyRail: View {
  let icon: String
  let title: String
  let message: String

  var body: some View {
    ContentUnavailableView {
      Label(title, systemImage: icon)
    } description: {
      Text(message)
    }
    .frame(maxWidth: .infinity, minHeight: 320)
  }
}

private struct RetireMemorySheet: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.dismiss) private var dismiss
  let lesson: MemoryLesson
  @State private var reason = ""
  @State private var reviewOwner = ""

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      HStack(spacing: 12) {
        Image(systemName: "clock.arrow.circlepath")
          .font(.title2)
          .foregroundStyle(.orange)
        VStack(alignment: .leading, spacing: 3) {
          Text("That’s outdated").font(.title2.weight(.semibold))
          Text("Stop this lesson from guiding future work.").foregroundStyle(.secondary)
        }
      }
      Text(lesson.summary)
        .font(.body.weight(.medium))
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 10))
      Text("MDx will preserve the original decision and this retirement as receipts. Nothing is silently erased.")
        .font(.callout)
        .foregroundStyle(.secondary)
      Form {
        TextField("What changed?", text: $reason, axis: .vertical)
          .lineLimit(3...5)
        TextField("Review owner", text: $reviewOwner)
      }
      HStack {
        Spacer()
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
        Button("Retire lesson") {
          Task {
            await store.retireMemoryLesson(
              lesson,
              reason: reason.trimmingCharacters(in: .whitespacesAndNewlines),
              reviewOwner: reviewOwner.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            if store.memoryActionResult?.succeeded == true { dismiss() }
          }
        }
        .buttonStyle(.borderedProminent)
        .keyboardShortcut(.defaultAction)
        .disabled(reason.trimmingCharacters(in: .whitespacesAndNewlines).count < 12 || reviewOwner.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.memoryActionInFlight)
      }
    }
    .padding(24)
    .frame(width: 520)
    .onAppear { reviewOwner = lesson.reviewOwner }
  }
}
