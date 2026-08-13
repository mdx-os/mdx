import SwiftUI

struct CloudGateView<Content: View>: View {
  @Environment(CloudAuthStore.self) private var auth
  @ViewBuilder let content: () -> Content

  var body: some View {
    Group {
      if !auth.requiresCloudSignIn || auth.state == .authorized {
        content()
      } else {
        gate
      }
    }
    .task { await auth.restore() }
  }

  private var gate: some View {
    VStack(spacing: 22) {
      RocketMark(size: 52)
      VStack(spacing: 8) {
        Text(title).font(.system(size: 30, weight: .bold, design: .rounded))
        Text(detail)
          .foregroundStyle(.secondary)
          .multilineTextAlignment(.center)
          .frame(maxWidth: 430)
      }
      switch auth.state {
      case .signedOut:
        VStack(spacing: 12) {
          Button("Continue with Google") { Task { await auth.signInWithGoogle() } }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
          Button {
            Task { await auth.signInWithApple() }
          } label: {
            HStack(spacing: 7) {
              Image(systemName: "apple.logo")
              Text("Continue with Apple")
            }
              .font(.system(size: 14, weight: .medium))
              .foregroundStyle(.white)
              .frame(width: 206, height: 24)
          }
          .buttonStyle(.borderedProminent)
          .tint(.black)
          .controlSize(.large)
          .accessibilityLabel("Continue with Apple")
        }
      case .pending:
        HStack {
          Button("Check access") { Task { await auth.recheckAccess() } }
            .buttonStyle(.borderedProminent)
          Button("Sign out") { Task { await auth.signOut() } }
        }
      case .failed:
        HStack {
          Button("Try again") { Task { await auth.recheckAccess() } }
            .buttonStyle(.borderedProminent)
          Button("Sign out") { Task { await auth.signOut() } }
        }
      default:
        ProgressView().controlSize(.large)
      }
      if !auth.email.isEmpty { Text(auth.email).font(.caption).foregroundStyle(.tertiary) }
    }
    .padding(48)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(.background)
  }

  private var title: String {
    switch auth.state {
    case .pending: "Your beta access is pending"
    case .failed: "We could not verify access"
    case .signedOut: "Sign in to MDx"
    default: "Opening MDx"
    }
  }

  private var detail: String {
    switch auth.state {
    case .pending: "You are signed in, but this account has not been admitted to the canary yet. Redeem your invite on the MDx website, then check again."
    case .failed(let message): message
    case .signedOut: "Use the Google or Apple account connected to your private beta invite."
    default: "Restoring your secure session and checking beta admission."
    }
  }
}
