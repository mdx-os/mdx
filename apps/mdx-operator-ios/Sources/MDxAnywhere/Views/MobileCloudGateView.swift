import AuthenticationServices
import SwiftUI

struct MobileCloudGateView<Content: View>: View {
  @ObservedObject var auth: MobileCloudAuthStore
  @ViewBuilder let content: Content

  init(auth: MobileCloudAuthStore, @ViewBuilder content: () -> Content) {
    self.auth = auth
    self.content = content()
  }

  var body: some View {
    Group {
      switch auth.state {
      case .local, .authorized:
        content
      case .restoring, .checkingAccess:
        ProgressView("Connecting to MDx")
          .controlSize(.large)
      case .signedOut:
        signInCard(
          title: "Your builds, wherever you are",
          detail: "Use the same account you use for MDx on the web and Mac."
        )
      case .pending:
        signInCard(
          title: "Your account is waiting for access",
          detail:
            "This account is not in the private beta yet. Recheck after an invite is added.",
          action: "Recheck access",
          perform: { await auth.recheckAccess() },
          allowAccountChange: true
        )
      case .failed(let message):
        if auth.email.isEmpty {
          signInCard(
            title: "MDx could not finish connecting",
            detail: message
          )
        } else {
          signInCard(
            title: "MDx could not finish connecting",
            detail: message,
            action: "Try again",
            perform: { await auth.recheckAccess() },
            allowAccountChange: true
          )
        }
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  private func signInCard(
    title: String,
    detail: String,
    action: String = "Continue with Google",
    perform: (() async -> Void)? = nil,
    allowAccountChange: Bool = false
  ) -> some View {
    VStack(spacing: 18) {
      Image(systemName: "hammer.circle.fill")
        .font(.system(size: 58))
        .foregroundStyle(.tint)
      Text("MDx")
        .font(.largeTitle.bold())
      Text(title)
        .font(.title3.weight(.semibold))
        .multilineTextAlignment(.center)
      Text(detail)
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)
      if !auth.email.isEmpty {
        Text("Signed in as \(auth.email)")
          .font(.caption)
          .foregroundStyle(.secondary)
          .multilineTextAlignment(.center)
      }
      Button(action) {
        Task {
          if let perform { await perform() } else { await auth.signInWithGoogle() }
        }
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.large)
      if perform == nil {
        SignInWithAppleButton(.continue) { request in
          auth.prepareAppleRequest(request)
        } onCompletion: { result in
          Task { await auth.signInWithApple(result) }
        }
        .signInWithAppleButtonStyle(.black)
        .frame(height: 50)
        .accessibilityLabel("Continue with Apple")
      }
      if allowAccountChange {
        Button("Use a different account") {
          Task { await auth.signOut() }
        }
        .buttonStyle(.bordered)
        .accessibilityIdentifier("auth.change-account")
      }
    }
    .padding(32)
    .frame(maxWidth: 440)
  }
}
