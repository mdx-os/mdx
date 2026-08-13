import UserNotifications

final class NotificationService: UNNotificationServiceExtension {
  private var completion: ((UNNotificationContent) -> Void)?
  private var bestAttempt: UNMutableNotificationContent?

  override func didReceive(
    _ request: UNNotificationRequest,
    withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
  ) {
    completion = contentHandler
    bestAttempt = request.content.mutableCopy() as? UNMutableNotificationContent
    guard let bestAttempt else {
      contentHandler(request.content)
      return
    }

    let allowed = Set(["session_ref", "event_ref", "category"])
    bestAttempt.userInfo = bestAttempt.userInfo.filter {
      allowed.contains(String(describing: $0.key))
    }
    contentHandler(bestAttempt)
  }

  override func serviceExtensionTimeWillExpire() {
    if let completion, let bestAttempt {
      completion(bestAttempt)
    }
  }
}
