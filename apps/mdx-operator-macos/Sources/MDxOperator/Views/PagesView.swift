import AppKit
import SwiftUI

struct PagesView: View {
  @Environment(OperatorStore.self) private var store
  @State private var showRail = true

  var body: some View {
    HStack(spacing: 0) {
      if showRail {
        libraryRail
          .frame(width: SurfaceMetrics.railWidth)
          .transition(.move(edge: .leading).combined(with: .opacity))
        Divider()
      }
      readerColumn
    }
    .task {
      if store.pages.isEmpty { await store.loadPages() }
    }
    .sheet(isPresented: Bindable(store).showPagePublish) {
      PagePublishSheet()
        .environment(store)
    }
  }

  // MARK: - Library rail

  private var railEmptyText: String {
    if !store.pageSearch.isEmpty { return "No pages match “\(store.pageSearch)”." }
    switch store.pagesPhase {
    case .failed(let message), .stale(let message): return message
    default: return "No pages yet. Publish one to start your team library."
    }
  }

  private var libraryRail: some View {
    VStack(spacing: 0) {
      RailHeader(title: "Pages", count: store.pages.count + store.pageDraftCount)

      SurfaceSearchField(placeholder: "Search pages", text: Bindable(store).pageSearch) { value in
        store.runPageSearch(value)
      }
      .padding(.horizontal, 10)
      .padding(.bottom, 8)

      Divider()

      ScrollView {
        LazyVStack(alignment: .leading, spacing: 2) {
          if store.visiblePages.isEmpty {
            if store.pagesPhase == .idle || store.pagesPhase == .loading {
              // Don't flash an empty state before the first load resolves.
              RailSkeleton()
            } else {
              Text(railEmptyText)
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(10)
            }
          }
          if !store.visiblePageDrafts.isEmpty && store.pageSearch.isEmpty {
            PageRailSectionLabel(title: "Work in progress", count: store.visiblePageDrafts.count)
            ForEach(store.visiblePageDrafts) { draft in
              PageDraftRow(draft: draft, review: store.pageReviewRequests.last { $0.draftID == draft.id }) {
                store.openPageDraft(draft)
              }
            }
            PageRailSectionLabel(title: "Published", count: store.pages.count)
          }
          ForEach(store.visiblePages) { page in
            PageRow(page: page, selected: page.id == store.selectedPageID) {
              store.selectPage(page.id)
            }
          }
        }
        .padding(8)
      }
    }
    .background(Color.primary.opacity(0.025))
  }

  // MARK: - Reader

  private var readerColumn: some View {
    VStack(spacing: 0) {
      readerTopBar
      Divider()
      if let result = store.pagePublishResult, result.succeeded, !result.receiptID.isEmpty {
        PagePublishResultBanner(result: result) { store.select(.memory) }
        Divider()
      }
      if let page = store.selectedPage {
        PageReader(page: page, pageBody: store.selectedPageBody, bodyPhase: store.pageBodyPhase)
      } else {
        SurfaceDetailPlaceholder(
          connected: store.snapshot.connectionStatus == .ok,
          phase: store.pagesPhase,
          isEmpty: store.pages.isEmpty,
          icon: "doc.richtext",
          selectPrompt: "Select a page to read it",
          emptyTitle: "No pages yet",
          emptyMessage: "Pages hold the context and knowledge your team and Twin can cite. Publish the first one to get started.",
          actionLabel: "New page",
          action: { store.startNewPage() },
          retry: { Task { await store.refresh() } }
        )
      }
    }
  }

  private var readerTopBar: some View {
    SurfaceTopBar(icon: "doc.richtext", title: "Pages",
          subtitle: "draft, review, and publish knowledge", showRail: $showRail) {
      if let page = store.selectedPage {
        ShareLink(
          item: store.selectedPageBody?.body ?? page.title,
          subject: Text(page.title),
          preview: SharePreview(page.title)
        ) {
          Label("Share", systemImage: "square.and.arrow.up")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Share this page as text")

        Button {
          store.startEditPage()
        } label: {
          Label("Edit", systemImage: "square.and.pencil")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Edit and republish this page")
      }
      Button {
        store.startNewPage()
      } label: {
        Label("New page", systemImage: "plus")
          .font(.callout.weight(.medium))
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.small)
      .help("Write and publish a new page")
    }
  }
}

struct PageDraftRow: View {
  let draft: PageDraftRecord
  let review: PageReviewRequest?
  let open: () -> Void

  var body: some View {
    Button(action: open) {
      VStack(alignment: .leading, spacing: 4) {
        Text(draft.title)
          .font(.subheadline.weight(.medium))
          .foregroundStyle(.primary)
          .lineLimit(2)
        HStack(spacing: 6) {
          StatusPill(
            status: review?.isApproved == true ? "Approved" : (review?.isPending == true ? "In review" : (review?.isRejected == true ? "Needs work" : "Draft")),
            color: review?.isApproved == true ? .blue : (review?.isPending == true ? .orange : .secondary)
          )
          Spacer(minLength: 0)
          Image(systemName: "chevron.right")
            .font(.caption2)
            .foregroundStyle(.tertiary)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.horizontal, 10)
      .padding(.vertical, 8)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
    .buttonStyle(.plain)
    .help("Inspect the recorded draft and its receipt")
  }
}

private struct PageRailSectionLabel: View {
  let title: String
  let count: Int

  var body: some View {
    HStack {
      Text(title.uppercased())
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.tertiary)
      Spacer()
      Text("\(count)")
        .font(.caption2.monospacedDigit())
        .foregroundStyle(.tertiary)
    }
    .padding(.horizontal, 10)
    .padding(.top, 12)
    .padding(.bottom, 4)
  }
}

struct PagePublishResultBanner: View {
  let result: RunActionOutcome
  let openMemory: () -> Void

  var body: some View {
    HStack(spacing: 10) {
      Label("Published", systemImage: "checkmark.seal")
        .font(.caption.weight(.semibold))
        .foregroundStyle(.green)
      Text("MDx will remember this page.")
        .font(.caption)
        .foregroundStyle(.secondary)
      Text(result.receiptID)
        .font(.caption2.monospaced())
        .foregroundStyle(.tertiary)
        .lineLimit(1)
        .truncationMode(.middle)
        .textSelection(.enabled)
      Spacer(minLength: 0)
      Button("See it in Memory", action: openMemory)
        .buttonStyle(.plain)
        .foregroundStyle(Color.accentColor)
        .help("MDx will remember this page after the recorded publication")
      Button {
        Pasteboard.copy(result.receiptID)
      } label: {
        Image(systemName: "doc.on.doc")
      }
      .buttonStyle(.plain)
      .help("Copy receipt")
      .accessibilityLabel("Copy publication receipt")
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 8)
    .background(Color.green.opacity(0.08))
  }
}

// MARK: - Rail row

struct PageRow: View {
  let page: Page
  let selected: Bool
  let open: () -> Void
  @State private var hovering = false

  var body: some View {
    Button(action: open) {
      VStack(alignment: .leading, spacing: 4) {
        Text(page.title)
          .font(.subheadline.weight(.medium))
          .foregroundStyle(.primary)
          .lineLimit(2)
        HStack(spacing: 6) {
          StateBadge(state: page.state)
          if page.trustedForAI {
            Label("Trusted", systemImage: "checkmark.seal.fill")
              .labelStyle(.iconOnly)
              .font(.caption2)
              .foregroundStyle(.green)
              .help("Trusted for AI use")
          }
          if page.stale {
            Image(systemName: "clock.badge.exclamationmark")
              .font(.caption2)
              .foregroundStyle(.orange)
              .help("Review is overdue")
          }
          Spacer(minLength: 0)
          Text(page.authorLabel)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(1)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.horizontal, 10)
      .padding(.vertical, 8)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(selected ? Color.accentColor.opacity(0.14) : (hovering ? Color.primary.opacity(0.05) : Color.clear))
      )
    }
    .buttonStyle(.plain)
    .accessibilityRemoveTraits(.isSelected)
    .accessibilityAddTraits(selected ? .isSelected : [])
    .onHover { hovering = $0 }
    .contextMenu {
      Button("Open") { open() }
      Divider()
      Button("Copy Title") { Pasteboard.copy(page.title) }
      Button("Copy Page ID") { Pasteboard.copy(page.id) }
    }
  }
}

struct StateBadge: View {
  let state: String

  private var color: Color {
    switch state {
    case "published": return .green
    case "approved": return .blue
    case "in_review": return .orange
    case "draft": return .secondary
    default: return .secondary
    }
  }

  var body: some View {
    if !state.isEmpty {
      // Folds into the app-wide StatusPill capsule, keeping page-state hues.
      StatusPill(status: PageState.label(state), color: color)
    }
  }
}

// MARK: - Reader

struct PageReader: View {
  let page: Page
  let pageBody: PageBody?
  let bodyPhase: LoadPhase

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 18) {
        header
        Divider()
        bodyContent
        if !page.sourceReceiptIDs.isEmpty || !page.charterEvidenceIDs.isEmpty || !page.events.isEmpty {
          Divider()
          provenance
        }
      }
      .padding(28)
      .frame(maxWidth: SurfaceMetrics.readingWidth, alignment: .leading)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text(page.title)
        .font(.largeTitle.weight(.semibold))
        .fixedSize(horizontal: false, vertical: true)
      HStack(spacing: 10) {
        StateBadge(state: page.state)
        PageMetaChip(icon: "person", text: page.ownerLabel, help: "Owner: \(page.owner.isEmpty ? page.author : page.owner)")
        PageMetaChip(icon: "eye", text: page.visibilityLabel, help: "Visibility: \(page.visibility)")
        if page.trustedForAI {
          PageMetaChip(icon: "checkmark.seal.fill", text: "Trusted for AI", tint: .green, help: "This page can ground AI answers.")
        }
        if page.stale {
          PageMetaChip(icon: "clock.badge.exclamationmark", text: "Review due", tint: .orange, help: "The review interval has lapsed.")
        }
      }
    }
  }

  @ViewBuilder
  private var bodyContent: some View {
    if let pageBody, !pageBody.body.isEmpty {
      MarkdownText(text: pageBody.body)
    } else {
      switch bodyPhase {
      case .loading, .idle:
        VStack(alignment: .leading, spacing: 12) {
          ForEach(0..<5, id: \.self) { index in
            RoundedRectangle(cornerRadius: 4)
              .fill(Color.primary.opacity(0.05))
              .frame(width: index.isMultiple(of: 3) ? 320 : 460, height: 11)
          }
        }
        .accessibilityElement()
        .accessibilityLabel("Loading page")
      case .failed(let message), .stale(let message):
        Text(message).font(SurfaceType.callout).foregroundStyle(.secondary)
      case .loaded, .empty:
        Text("This page has no body content.").font(SurfaceType.callout).foregroundStyle(.secondary)
      }
    }
  }

  private var provenance: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Where this came from")
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(.secondary)

      if !page.events.isEmpty {
        VStack(alignment: .leading, spacing: 6) {
          ForEach(page.events) { event in
            HStack(spacing: 8) {
              Image(systemName: "checkmark.seal")
                .font(.caption)
                .foregroundStyle(.tertiary)
              Text(event.label)
                .font(.caption.weight(.medium))
              if !event.actorID.isEmpty {
                Text("by \(Page.humanActor(event.actorID))")
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
              Spacer(minLength: 0)
            }
            .help(event.receiptID.isEmpty ? event.label : "Receipt: \(event.receiptID)")
          }
        }
      }

      if !page.sourceReceiptIDs.isEmpty {
        ProvenanceGroup(
          title: "\(page.sourceReceiptIDs.count) source receipt\(page.sourceReceiptIDs.count == 1 ? "" : "s")",
          items: page.sourceReceiptIDs,
          icon: "doc.badge.gearshape"
        )
      }
      if !page.charterEvidenceIDs.isEmpty {
        ProvenanceGroup(
          title: "\(page.charterEvidenceIDs.count) charter evidence link\(page.charterEvidenceIDs.count == 1 ? "" : "s")",
          items: page.charterEvidenceIDs,
          icon: "seal"
        )
      }
    }
  }
}

struct PageMetaChip: View {
  let icon: String
  let text: String
  var tint: Color = .secondary
  var help: String = ""

  var body: some View {
    Label(text, systemImage: icon)
      .font(.caption)
      .foregroundStyle(tint == .secondary ? .secondary : tint)
      .help(help.isEmpty ? text : help)
  }
}

struct ProvenanceGroup: View {
  let title: String
  let items: [String]
  let icon: String
  @State private var expanded = false

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      Button {
        withAnimation(.easeOut(duration: 0.12)) { expanded.toggle() }
      } label: {
        HStack(spacing: 6) {
          Image(systemName: icon).font(.caption)
          Text(title).font(.caption.weight(.medium))
          Image(systemName: expanded ? "chevron.down" : "chevron.right").font(.caption2)
        }
        .foregroundStyle(.secondary)
      }
      .buttonStyle(.plain)

      if expanded {
        VStack(alignment: .leading, spacing: 2) {
          ForEach(items, id: \.self) { item in
            Text(item)
              .font(.caption2.monospaced())
              .foregroundStyle(.tertiary)
              .textSelection(.enabled)
              .lineLimit(1)
              .truncationMode(.middle)
          }
        }
        .padding(.leading, 20)
      }
    }
  }
}

struct PagePublishSheet: View {
  @Environment(OperatorStore.self) private var store

  private var draft: Binding<PagePublishDraft> { Bindable(store).pagePublishDraft }

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack {
        Text(store.pagePublishDraft.isEdit ? "Edit page" : "New page")
          .font(.headline)
        Spacer()
        Text("Draft, review, then publish")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      .padding(20)
      Divider()

      ScrollView {
        VStack(alignment: .leading, spacing: 16) {
          field("Title") {
            TextField("What is this page about?", text: draft.title)
              .textFieldStyle(.plain)
              .font(.title3)
          }

          HStack(spacing: 16) {
            field("Type") {
              Picker("", selection: draft.pageType) {
                ForEach(PagePublishDraft.pageTypes, id: \.self) { type in
                  Text(PagePublishDraft.typeLabel(type)).tag(type)
                }
              }
              .labelsHidden()
              .fixedSize()
            }
            field("Visibility") {
              Picker("", selection: draft.visibility) {
                ForEach(PagePublishDraft.visibilities, id: \.self) { vis in
                  Text(PagePublishDraft.visibilityLabel(vis)).tag(vis)
                }
              }
              .labelsHidden()
              .fixedSize()
            }
            Spacer()
          }

          field("Body") {
            TextEditor(text: draft.body)
              .font(.system(.body, design: .default))
              .frame(minHeight: 240)
              .padding(8)
              .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                  .fill(Color.primary.opacity(0.04))
              )
              .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                  .stroke(Color.secondary.opacity(0.15), lineWidth: 1)
              )
            Text("Markdown. Everything you publish is receipt-backed and stays inspectable.")
              .font(.caption2)
              .foregroundStyle(.tertiary)
          }

          reviewWorkflow

          if let result = store.pagePublishResult {
            Label(result.detail, systemImage: result.succeeded ? "checkmark.seal" : "exclamationmark.triangle")
              .font(.caption)
              .foregroundStyle(result.succeeded ? .green : .orange)
              .fixedSize(horizontal: false, vertical: true)
            if !result.receiptID.isEmpty {
              Text(result.receiptID)
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .textSelection(.enabled)
            }
          }
        }
        .padding(20)
      }

      Divider()
      HStack {
        Text(workflowFooter)
          .font(.caption2)
          .foregroundStyle(.tertiary)
        Spacer()
        Button("Cancel") { store.showPagePublish = false }
          .keyboardShortcut(.cancelAction)
        Button("Save draft") { store.savePageDraft() }
          .disabled(!store.pagePublishDraft.isValid || store.pagePublishInFlight)
        primaryActionButton
      }
      .padding(20)
    }
    .frame(width: 600, height: 650)
  }

  private var primaryActionLabel: String {
    if store.activePageReviewCoversDraft { return store.pagePublishDraft.isEdit ? "Publish update" : "Publish" }
    if store.activePageReview?.isApproved == true { return "Save changes first" }
    if store.activePageReview?.isPending == true { return "Waiting for review" }
    if store.activePageReview?.isRejected == true { return "Save changes first" }
    return "Ask for review"
  }

  @ViewBuilder
  private var primaryActionButton: some View {
    if primaryActionDisabled {
      Button(primaryActionLabel, action: primaryAction)
        .buttonStyle(.bordered)
        .disabled(true)
    } else {
      Button(primaryActionLabel, action: primaryAction)
        .buttonStyle(.borderedProminent)
        .keyboardShortcut(.defaultAction)
    }
  }

  private var primaryActionDisabled: Bool {
    if store.pagePublishInFlight || !store.pagePublishDraft.isValid { return true }
    if store.activePageReviewCoversDraft { return false }
    if store.activePageReview?.isApproved == true { return true }
    if store.activePageReview?.isPending == true { return true }
    if store.activePageReview?.isRejected == true { return true }
    return store.pagePublishDraft.draftReceiptID.isEmpty
  }

  private var workflowFooter: String {
    if store.activePageReviewCoversDraft { return "Approved. Publish uses the exact recorded draft." }
    if store.activePageReview?.isApproved == true { return "These words changed after approval. Save and review them again." }
    if store.activePageReview?.isPending == true { return "A human decision is required. Publication remains blocked." }
    if store.pagePublishDraft.draftReceiptID.isEmpty { return "Save the draft before asking for review." }
    return "Draft saved. Ask for review when the words are ready."
  }

  private func primaryAction() {
    if store.activePageReviewCoversDraft { store.publishPage() }
    else { store.requestPageReview() }
  }

  private var reviewWorkflow: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Label("Review", systemImage: "person.crop.circle.badge.checkmark")
          .font(.subheadline.weight(.semibold))
        Spacer()
        if let review = store.activePageReview {
          StatusPill(
            status: review.isApproved ? "Approved" : (review.isRejected ? "Needs work" : "In review"),
            color: review.isApproved ? .blue : (review.isPending ? .orange : .secondary)
          )
        } else {
          StatusPill(status: "Not requested", color: .secondary)
        }
      }
      Text("A recorded decision gates publication. When you work alone, you may approve your own saved draft. Saving changed words creates a new draft, so an old approval can never carry forward.")
        .font(.caption)
        .foregroundStyle(.secondary)
      if store.activePageReview?.isPending == true {
        HStack {
          Button("Send back") { store.decidePageReview(approve: false) }
          Button("Approve") { store.decidePageReview(approve: true) }
            .buttonStyle(.borderedProminent)
        }
      } else if store.activePageReview == nil, store.previousPageReview != nil {
        Text("An earlier review does not cover this draft. Save these words, then request a new decision.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.035)))
  }

  @ViewBuilder
  private func field<Content: View>(_ label: String, @ViewBuilder content: () -> Content) -> some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(label.uppercased())
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.tertiary)
      content()
    }
  }
}
