import LocalAuthentication

struct BiometricDecisionAuthorizer {
  func verify(reason: String) async throws {
    let context = LAContext()
    context.localizedCancelTitle = "Keep blocked"
    var error: NSError?
    guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) else {
      throw error ?? BiometricDecisionError.unavailable
    }
    guard
      try await context.evaluatePolicy(
        .deviceOwnerAuthenticationWithBiometrics,
        localizedReason: reason
      )
    else {
      throw BiometricDecisionError.notVerified
    }
  }
}

enum BiometricDecisionError: Error, LocalizedError {
  case unavailable
  case notVerified

  var errorDescription: String? {
    switch self {
    case .unavailable: "Face ID is not available for this decision. Review it on your Mac."
    case .notVerified: "Your identity was not verified. Nothing changed."
    }
  }
}
