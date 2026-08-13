import MDxOperatorShared
import SwiftUI

@main
struct MDxAnywhereApp: App {
  @UIApplicationDelegateAdaptor(PushNotificationAppDelegate.self) private var appDelegate
  @StateObject private var auth: MobileCloudAuthStore
  @StateObject private var store: ForgeAnywhereStore

  init() {
    let auth = MobileCloudAuthStore()
    _auth = StateObject(wrappedValue: auth)
    _store = StateObject(
      wrappedValue: ForgeAnywhereStore.fromLaunchEnvironment(
        accessTokenProvider: { await auth.accessToken() }
      )
    )
  }

  var body: some Scene {
    WindowGroup {
      MobileCloudGateView(auth: auth) {
        Group {
          if shouldOverrideDynamicTypeForUITest {
            RootTabView()
              .environment(\.dynamicTypeSize, .accessibility5)
          } else {
            RootTabView()
          }
        }
      }
      .environmentObject(store)
      .environmentObject(auth)
      .task {
        await auth.restore()
        if auth.state == .local || auth.state == .authorized {
          await store.connectIfConfigured()
        }
      }
      .onChange(of: auth.state) { _, state in
        switch state {
        case .authorized:
          Task { await store.connectIfConfigured() }
        case .checkingAccess, .failed:
          store.suspendCloudAccess()
        case .signedOut, .pending:
          store.closeAuthenticatedSession()
        case .local, .restoring:
          break
        }
      }
      .onOpenURL { url in
        Task { await store.handleCloudCallback(url) }
      }
      .onReceive(NotificationCenter.default.publisher(for: .mdxRemoteNotificationToken)) {
        notification in
        guard let token = notification.object as? String else { return }
        Task { await store.registerPushToken(token) }
      }
      .onReceive(NotificationCenter.default.publisher(for: .mdxRemoteNotificationFailure)) {
        notification in
        guard let error = notification.object as? Error else { return }
        store.recordPushFailure(error)
      }
    }
  }

  private var shouldOverrideDynamicTypeForUITest: Bool {
    #if DEBUG
      ProcessInfo.processInfo.environment["MDX_UI_TEST_DYNAMIC_TYPE"] == "accessibility5"
    #else
      false
    #endif
  }
}
