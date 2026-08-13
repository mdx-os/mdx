import SwiftUI

struct ForgeNextMoveCard: View {
  let move: ForgeNextMove
  let perform: () -> Void

  var body: some View {
    HStack(spacing: 14) {
      Image(systemName: move.systemImage)
        .font(.system(size: 16, weight: .semibold))
        .foregroundStyle(iconColor)
        .frame(width: 36, height: 36)
        .background(iconColor.opacity(0.12), in: Circle())

      VStack(alignment: .leading, spacing: 3) {
        Text(move.eyebrow.uppercased())
          .font(.caption2.weight(.bold))
          .tracking(0.8)
          .foregroundStyle(iconColor)
        Text(move.title)
          .font(.headline)
          .lineLimit(2)
        Text(move.detail)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(3)
        if move.additionalWaiting > 0 {
          Text("+\(move.additionalWaiting) more waiting")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
        }
      }

      Spacer(minLength: 14)

      Button(move.buttonTitle, action: perform)
        .buttonStyle(.borderedProminent)
        .controlSize(.regular)
    }
    .padding(15)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(iconColor.opacity(0.055))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(iconColor.opacity(0.18), lineWidth: 1)
    )
  }

  private var iconColor: Color {
    switch move.kind {
    case .decision, .review, .recover: .accentColor
    case .follow: .green
    case .start: .accentColor
    case .connect: .secondary
    }
  }
}

struct ForgeTrailMapView: View {
  let scale: String
  let title: String
  let detail: String
  let active: ForgeTrailStage

  var body: some View {
    HStack(spacing: 22) {
      VStack(alignment: .leading, spacing: 3) {
        Text("\(scale) trail".uppercased())
          .font(.caption2.weight(.bold))
          .tracking(0.8)
          .foregroundStyle(.secondary)
        Text(title)
          .font(.subheadline.weight(.semibold))
        Text(detail)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }
      .frame(maxWidth: 250, alignment: .leading)

      HStack(spacing: 0) {
        ForEach(ForgeTrailStage.allCases, id: \.rawValue) { stage in
          HStack(spacing: 0) {
            if stage != .plan {
              Rectangle()
                .fill(stage.rawValue <= active.rawValue ? Color.green.opacity(0.7) : Color.secondary.opacity(0.2))
                .frame(height: 1)
            }
            VStack(spacing: 5) {
              Circle()
                .fill(stage.rawValue < active.rawValue ? Color.green : stage == active ? Color.accentColor : Color(nsColor: .windowBackgroundColor))
                .overlay(Circle().stroke(stage == active ? Color.accentColor : Color.secondary.opacity(0.35), lineWidth: stage == active ? 2 : 1))
                .frame(width: 11, height: 11)
              Text(stage.title)
                .font(.caption2.weight(stage == active ? .semibold : .regular))
                .foregroundColor(stage.rawValue <= active.rawValue ? .primary : .secondary)
            }
            if stage != .review {
              Rectangle()
                .fill(stage.rawValue < active.rawValue ? Color.green.opacity(0.7) : Color.secondary.opacity(0.2))
                .frame(height: 1)
            }
          }
          .frame(maxWidth: .infinity)
        }
      }
    }
    .padding(13)
    .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.secondary.opacity(0.045)))
    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.secondary.opacity(0.12), lineWidth: 1))
  }
}
