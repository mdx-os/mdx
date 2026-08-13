import UIKit

extension Notification.Name {
  static let mdxRemoteNotificationToken = Notification.Name("mdx.remote-notification-token")
  static let mdxRemoteNotificationFailure = Notification.Name("mdx.remote-notification-failure")
}

final class PushNotificationAppDelegate: NSObject, UIApplicationDelegate {
  func application(
    _ application: UIApplication,
    didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
  ) {
    let token = deviceToken.map { String(format: "%02x", $0) }.joined()
    NotificationCenter.default.post(name: .mdxRemoteNotificationToken, object: token)
  }

  func application(
    _ application: UIApplication,
    didFailToRegisterForRemoteNotificationsWithError error: Error
  ) {
    NotificationCenter.default.post(name: .mdxRemoteNotificationFailure, object: error)
  }
}
