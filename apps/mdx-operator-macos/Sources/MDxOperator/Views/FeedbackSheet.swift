import SwiftUI

/// Report an issue (Cmd-Shift-B): a governed feedback capture that comes back
/// with a receipt. What the app attaches is said out loud, the categories are
/// the kernel's bounded set, and every report you've filed stays visible with
/// its receipt so you know it landed.
struct FeedbackSheet: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.dismiss) private var dismiss
  @FocusState private var noteFocused: Bool

  var body: some View {
    @Bindable var store = store
    return VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .firstTextBaseline) {
        Text("Report an issue")
          .font(.title2.weight(.semibold))
        Spacer()
        Text("Goes straight to the MDx team with a receipt")
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      HStack(spacing: 8) {
        ForEach(FeedbackCategory.all) { category in
          Button {
            store.feedbackCategoryID = category.id
          } label: {
            HStack(spacing: 5) {
              Image(systemName: category.icon)
              Text(category.label)
            }
            .font(.callout)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(
              RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(store.feedbackCategoryID == category.id ? Color.accentColor.opacity(0.14) : Color.primary.opacity(0.04))
            )
            .overlay(
              RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(store.feedbackCategoryID == category.id ? Color.accentColor.opacity(0.4) : Color.clear, lineWidth: 1)
            )
          }
          .buttonStyle(.plain)
        }
      }

      TextEditor(text: $store.feedbackNote)
        .font(.body)
        .scrollContentBackground(.hidden)
        .padding(8)
        .frame(minHeight: 110)
        .background(
          RoundedRectangle(cornerRadius: 9, style: .continuous)
            .fill(Color(nsColor: .textBackgroundColor))
        )
        .overlay(
          RoundedRectangle(cornerRadius: 9, style: .continuous)
            .stroke(Color.secondary.opacity(0.2), lineWidth: 1)
        )
        .focused($noteFocused)
        .onAppear { noteFocused = true }
        .overlay(alignment: .topLeading) {
          if store.feedbackNote.isEmpty {
            Text("What happened, and what did you expect instead?")
              .font(.body)
              .foregroundStyle(.tertiary)
              .padding(.horizontal, 12)
              .padding(.vertical, 16)
              .allowsHitTesting(false)
          }
        }

      Label(
        "Attached automatically: app version, current surface, urgency from the selected category, and health metrics (launch time, errors, hangs). Never your prompts, documents, or code.",
        systemImage: "info.circle"
      )
      .font(.caption)
      .foregroundStyle(.tertiary)

      if let result = store.feedbackResult {
        HStack(spacing: 8) {
          Image(systemName: result.succeeded ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
            .foregroundStyle(result.succeeded ? .green : .orange)
          if result.succeeded {
            Text("Recorded. Receipt \(result.receiptID)")
              .font(.callout)
            Button("Copy receipt") { Pasteboard.copy(result.receiptID) }
              .controlSize(.small)
          } else {
            Text(result.detail.isEmpty ? "That didn't record. Try again in a moment." : result.detail)
              .font(.callout)
              .foregroundStyle(.secondary)
          }
          Spacer(minLength: 0)
        }
        .padding(10)
        .background(
          RoundedRectangle(cornerRadius: 8, style: .continuous)
            .fill((result.succeeded ? Color.green : Color.orange).opacity(0.08))
        )
      }

      if !store.filedReports.isEmpty {
        VStack(alignment: .leading, spacing: 6) {
          Text("Your reports")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          ScrollView {
            VStack(alignment: .leading, spacing: 4) {
              ForEach(store.filedReports.prefix(8)) { report in
                HStack(spacing: 8) {
                  Text(report.categoryLabel)
                    .font(.caption.weight(.medium))
                  Text(report.note)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                  Spacer(minLength: 8)
                  if let remote = store.reportStatuses[report.receiptID] {
                    Text(remote.label)
                      .font(.caption2.weight(.medium))
                      .foregroundStyle(remote.isResolved ? Color.green : (remote.isActive ? Color.accentColor : Color.secondary))
                      .padding(.horizontal, 6)
                      .padding(.vertical, 1)
                      .background(
                        Capsule().fill((remote.isResolved ? Color.green : (remote.isActive ? Color.accentColor : Color.secondary)).opacity(0.12))
                      )
                      .help(remote.statusNote.isEmpty ? "Status from MDx" : remote.statusNote)
                  }
                  Text(report.filedAt.formatted(date: .abbreviated, time: .shortened))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                  Image(systemName: "checkmark.seal")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .help("Receipt: \(report.receiptID)")
                }
                .padding(.vertical, 2)
                .contextMenu {
                  Button("Copy Receipt ID") { Pasteboard.copy(report.receiptID) }
                }
              }
            }
          }
          .frame(maxHeight: 110)
        }
      }

      HStack(spacing: 10) {
        Spacer()
        Button("Close") { dismiss() }
          .keyboardShortcut(.cancelAction)
        Button {
          store.sendFeedback()
        } label: {
          Label(store.feedbackInFlight ? "Sending" : "Send report", systemImage: "paperplane.fill")
        }
        .mdxPrimaryButtonStyle()
        .keyboardShortcut(.defaultAction)
        .disabled(store.feedbackInFlight || store.feedbackNote.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
      }
    }
    .padding(20)
    .frame(minWidth: 540, idealWidth: 560, minHeight: 420)
  }
}
