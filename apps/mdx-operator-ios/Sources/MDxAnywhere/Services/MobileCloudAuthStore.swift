import AuthenticationServices
import CryptoKit
import Foundation
import Supabase

struct MobileCloudConfiguration: Equatable, Sendable {
  let supabaseURL: URL
  let publishableKey: String
  let hostedOrigin: URL
  let hostedAPIURL: URL
  let redirectURL: URL

  static var current: MobileCloudConfiguration? {
    func value(_ environment: String, _ plist: String) -> String {
      if let raw = ProcessInfo.processInfo.environment[environment]?
        .trimmingCharacters(in: .whitespacesAndNewlines), !raw.isEmpty
      {
        return raw
      }
      return String(Bundle.main.object(forInfoDictionaryKey: plist) as? String ?? "")
        .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    guard
      let supabaseURL = URL(string: value("MDX_SUPABASE_URL", "MDXSupabaseURL")),
      let hostedOrigin = URL(string: value("MDX_PUBLIC_HOST_URL", "MDXPublicHostURL")),
      supabaseURL.scheme == "https",
      hostedOrigin.scheme == "https"
    else { return nil }

    let key = value("MDX_SUPABASE_PUBLISHABLE_KEY", "MDXSupabasePublishableKey")
    guard !key.isEmpty else { return nil }

    return MobileCloudConfiguration(
      supabaseURL: supabaseURL,
      publishableKey: key,
      hostedOrigin: hostedOrigin,
      hostedAPIURL: hostedOrigin.appending(path: "api/kernel"),
      redirectURL: URL(string: "mdx://auth/callback")!
    )
  }
}

@MainActor
final class MobileCloudAuthStore: ObservableObject {
  enum State: Equatable {
    case local
    case restoring
    case signedOut
    case checkingAccess
    case pending
    case authorized
    case failed(String)
  }

  @Published private(set) var state: State
  @Published private(set) var email = ""
  @Published private(set) var displayName = ""

  let configuration: MobileCloudConfiguration?
  private let client: SupabaseClient?
  private var appleNonce: String?

  init(configuration: MobileCloudConfiguration? = .current) {
    self.configuration = configuration
    if let configuration {
      client = SupabaseClient(
        supabaseURL: configuration.supabaseURL,
        supabaseKey: configuration.publishableKey,
        options: SupabaseClientOptions(
          auth: .init(
            storage: KeychainLocalStorage(service: "com.mdx.anywhere.supabase"),
            redirectToURL: configuration.redirectURL,
            flowType: .pkce,
            autoRefreshToken: true,
            emitLocalSessionAsInitialSession: true
          )
        )
      )
      state = .restoring
    } else {
      client = nil
      state = .local
    }
  }

  var requiresCloudSignIn: Bool { configuration != nil }

  func restore() async {
    guard let client else { return }
    state = .restoring
    do {
      let session = try await client.auth.session
      applyIdentity(session.user)
      await verifyAdmission(session.accessToken)
    } catch {
      state = .signedOut
    }
  }

  func signInWithGoogle() async {
    guard let client, let configuration else { return }
    state = .checkingAccess
    do {
      let session = try await client.auth.signInWithOAuth(
        provider: .google,
        redirectTo: configuration.redirectURL
      ) { browserSession in
        browserSession.prefersEphemeralWebBrowserSession = false
      }
      applyIdentity(session.user)
      await verifyAdmission(session.accessToken)
    } catch let error as ASWebAuthenticationSessionError where error.code == .canceledLogin {
      state = .signedOut
    } catch {
      state = .failed("Sign-in did not complete. Check your connection and try again.")
    }
  }

  func prepareAppleRequest(_ request: ASAuthorizationAppleIDRequest) {
    let nonce = UUID().uuidString
    appleNonce = nonce
    request.requestedScopes = [.email, .fullName]
    request.nonce = SHA256.hash(data: Data(nonce.utf8))
      .map { String(format: "%02x", $0) }
      .joined()
  }

  func signInWithApple(_ result: Result<ASAuthorization, Error>) async {
    guard let client else { return }
    guard let nonce = appleNonce else {
      state = .failed("Apple sign-in could not be verified. Please try again.")
      return
    }
    appleNonce = nil
    state = .checkingAccess
    do {
      let authorization = try result.get()
      guard
        let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
        let tokenData = credential.identityToken,
        let idToken = String(data: tokenData, encoding: .utf8)
      else {
        state = .failed("Apple did not return a usable identity. Please try again.")
        return
      }

      let session = try await client.auth.signInWithIdToken(
        credentials: OpenIDConnectCredentials(provider: .apple, idToken: idToken, nonce: nonce)
      )
      if let fullName = credential.fullName?.formatted(), !fullName.isEmpty {
        _ = try? await client.auth.update(
          user: UserAttributes(data: ["full_name": .string(fullName)])
        )
      }
      applyIdentity(session.user, preferredName: credential.fullName?.formatted())
      await verifyAdmission(session.accessToken)
    } catch let error as ASAuthorizationError where error.code == .canceled {
      state = .signedOut
    } catch {
      state = .failed("Sign in with Apple did not complete. Please try again.")
    }
  }

  func recheckAccess() async {
    guard let token = await accessToken() else {
      state = .signedOut
      return
    }
    await verifyAdmission(token)
  }

  func signOut() async {
    do { try await client?.auth.signOut() } catch {}
    email = ""
    displayName = ""
    state = .signedOut
  }

  func accessToken() async -> String? {
    guard let client else { return nil }
    do { return try await client.auth.session.accessToken } catch { return nil }
  }

  private func verifyAdmission(_ token: String) async {
    guard let configuration else { return }
    state = .checkingAccess
    var request = URLRequest(url: configuration.hostedAPIURL.appending(path: "version.json"))
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    request.timeoutInterval = 10
    do {
      let (_, response) = try await URLSession.shared.data(for: request)
      guard let response = response as? HTTPURLResponse else {
        state = .failed("MDx returned an invalid access response.")
        return
      }
      switch response.statusCode {
      case 200..<300: state = .authorized
      case 401, 403: state = .pending
      default: state = .failed("MDx access is temporarily unavailable.")
      }
    } catch {
      state = .failed("MDx could not verify access. Check your connection and try again.")
    }
  }

  private func applyIdentity(_ user: User, preferredName: String? = nil) {
    email = user.email ?? ""
    let candidate = ["full_name", "name", "display_name"]
      .compactMap { user.userMetadata[$0]?.stringValue }
      .first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    displayName = preferredName ?? candidate ?? emailName
  }

  private var emailName: String {
    let local = email.split(separator: "@").first.map(String.init) ?? ""
    return
      local
      .replacingOccurrences(of: ".", with: " ")
      .replacingOccurrences(of: "_", with: " ")
      .split(separator: " ")
      .map { $0.capitalized }
      .joined(separator: " ")
  }
}
