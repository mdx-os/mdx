import XCTest

final class MobileStateUITests: XCTestCase {
  @MainActor
  private func launch(_ fixture: String) -> XCUIApplication {
    let app = XCUIApplication()
    app.launchEnvironment["MDX_FIXTURE_SCENARIO"] = fixture
    app.launch()
    return app
  }

  @MainActor
  private func keepScreenshot(_ app: XCUIApplication, named name: String) {
    let attachment = XCTAttachment(screenshot: app.screenshot())
    attachment.name = name
    attachment.lifetime = .keepAlways
    add(attachment)
  }

  @MainActor
  private func reveal(_ element: XCUIElement, in app: XCUIApplication, attempts: Int = 8) {
    for _ in 0..<attempts where !element.isHittable {
      app.swipeUp()
    }
  }

  @MainActor
  func testRequiredStatesRenderWithoutLosingPrimaryNavigation() {
    for fixture in ["loading", "empty", "offline", "active", "needs_you", "review_ready", "error"] {
      let app = launch(fixture)

      XCTAssertTrue(app.tabBars.buttons["Now"].waitForExistence(timeout: 3), fixture)
      XCTAssertTrue(app.tabBars.buttons["Forge"].exists, fixture)
      XCTAssertTrue(app.tabBars.buttons["Review"].exists, fixture)
      XCTAssertTrue(app.tabBars.buttons["You"].exists, fixture)
      app.terminate()
    }
  }

  @MainActor
  func testLivePairedForgeStartAndReconnectWhenRequested() throws {
    #if !MDX_LIVE_MOBILE_ACCEPTANCE
      throw XCTSkip("Compile with MDX_LIVE_MOBILE_ACCEPTANCE to exercise the paired live relay.")
    #else

      let marker = UUID().uuidString.prefix(8)
      let goal =
        "Add a short mobile acceptance note to README.md with marker \(marker). Keep existing behavior unchanged."
      let app = XCUIApplication()
      app.launchEnvironment["MDX_API_URL"] = "http://127.0.0.1:18891"
      app.launch()

      XCTAssertTrue(app.tabBars.buttons["Now"].waitForExistence(timeout: 10))
      let connectedStatus = app.staticTexts["now.status"]
      XCTAssertTrue(connectedStatus.waitForExistence(timeout: 10))
      let connected = NSPredicate(format: "label BEGINSWITH %@", "Forge is connected")
      expectation(for: connected, evaluatedWith: connectedStatus)
      waitForExpectations(timeout: 20)
      app.tabBars.buttons["Forge"].tap()
      XCTAssertTrue(app.navigationBars["Forge"].waitForExistence(timeout: 5))
      let repositoryPicker = app.descendants(matching: .any)["forge.repository"]
      XCTAssertTrue(repositoryPicker.waitForExistence(timeout: 20))
      repositoryPicker.tap()
      let playground = app.buttons["MDx Playground"]
      XCTAssertTrue(playground.waitForExistence(timeout: 5))
      playground.tap()
      let proofPlan = app.staticTexts["forge.proof-plan"]
      XCTAssertTrue(proofPlan.waitForExistence(timeout: 20))
      XCTAssertTrue(
        proofPlan.label.contains("Proof: npm test"),
        "Mobile should disclose and submit the selected repository proof command."
      )

      let brief = app.textFields["forge.intent"]
      XCTAssertTrue(brief.waitForExistence(timeout: 5))
      brief.tap()
      brief.typeText(goal)
      app.toolbars.buttons["Done"].tap()

      let startBuild = app.buttons["forge.start-build"]
      reveal(startBuild, in: app)
      let enabled = NSPredicate(format: "enabled == true")
      expectation(for: enabled, evaluatedWith: startBuild)
      waitForExpectations(timeout: 30)
      XCTAssertTrue(
        startBuild.isEnabled, "A paired phone with a selected repository should be ready to start.")
      startBuild.tap()
      let briefCleared = NSPredicate(format: "value != %@", goal)
      expectation(for: briefCleared, evaluatedWith: brief)
      waitForExpectations(timeout: 60)
      XCTAssertTrue(app.descendants(matching: .any)["forge.feedback"].waitForExistence(timeout: 30))
      keepScreenshot(app, named: "live-01-command-accepted")

      app.tabBars.buttons["Now"].tap()
      var liveSession = app.descendants(matching: .any).matching(
        NSPredicate(
          format: "identifier BEGINSWITH %@ AND label CONTAINS %@",
          "now.session.forge_run_mobile_", goal
        )
      ).firstMatch
      XCTAssertTrue(liveSession.waitForExistence(timeout: 20))
      XCTAssertTrue(liveSession.label.contains(goal))
      keepScreenshot(app, named: "live-02-session-visible")

      let readyForReview = NSPredicate(format: "label CONTAINS %@", "Ready to review")
      expectation(for: readyForReview, evaluatedWith: liveSession)
      waitForExpectations(timeout: 120)

      app.terminate()
      app.launch()
      XCTAssertTrue(app.tabBars.buttons["Now"].waitForExistence(timeout: 10))
      liveSession =
        app.descendants(matching: .any).matching(
          NSPredicate(
            format: "identifier BEGINSWITH %@ AND label CONTAINS %@",
            "now.session.forge_run_mobile_", goal
          )
        ).firstMatch
      XCTAssertTrue(liveSession.waitForExistence(timeout: 20))
      XCTAssertTrue(liveSession.label.contains(goal))
      keepScreenshot(app, named: "live-03-session-after-reconnect")

      let sessionID = liveSession.identifier.replacingOccurrences(of: "now.session.", with: "")
      liveSession.tap()
      XCTAssertTrue(
        app.descendants(matching: .any)["session.timeline"].waitForExistence(timeout: 10))
      keepScreenshot(app, named: "live-04-session-timeline")

      app.terminate()
      app.launch()
      XCTAssertTrue(app.tabBars.buttons["Review"].waitForExistence(timeout: 10))
      app.tabBars.buttons["Review"].tap()
      let review = app.descendants(matching: .any)["review.digest.mobile_review_\(sessionID)"]
      XCTAssertTrue(review.waitForExistence(timeout: 20))
      XCTAssertTrue(
        app.staticTexts["Review the change, then decide: ship, ask a revision, or stop."]
          .waitForExistence(timeout: 30)
      )
      keepScreenshot(app, named: "live-05-review-digest")

      let focusedDiff = app.buttons["review.diff.mobile_review_\(sessionID)"]
      XCTAssertTrue(focusedDiff.waitForExistence(timeout: 10))
      focusedDiff.tap()
      XCTAssertTrue(app.navigationBars["Focused diff"].waitForExistence(timeout: 10))
      let readme = app.staticTexts["README.md"]
      XCTAssertTrue(readme.waitForExistence(timeout: 10))
      readme.tap()
      XCTAssertTrue(app.navigationBars["README.md"].waitForExistence(timeout: 10))
      let patchText = app.staticTexts.matching(
        NSPredicate(format: "label CONTAINS %@", String(marker))
      ).firstMatch
      XCTAssertTrue(patchText.waitForExistence(timeout: 10))
      keepScreenshot(app, named: "live-06-focused-diff")
    #endif
  }

  @MainActor
  func testAccessibilityTextKeepsStartBuildReachable() {
    let app = XCUIApplication()
    app.launchEnvironment["MDX_FIXTURE_SCENARIO"] = "empty"
    app.launchEnvironment["MDX_UI_TEST_DYNAMIC_TYPE"] = "accessibility5"
    app.launch()

    let startBuild = app.buttons["Start build"]
    XCTAssertTrue(startBuild.waitForExistence(timeout: 3))
    reveal(startBuild, in: app)
    XCTAssertTrue(startBuild.isHittable)
  }

  @MainActor
  func testSystemContentSizeKeepsStartBuildReachable() {
    let app = launch("empty")
    let startBuild = app.buttons["Start build"]
    XCTAssertTrue(startBuild.waitForExistence(timeout: 3))
    reveal(startBuild, in: app)
    XCTAssertTrue(startBuild.isHittable)
    keepScreenshot(app, named: "system-content-size-start-build")
  }

  @MainActor
  func testReviewReadyActionOpensTheReviewWorkspace() {
    let app = launch("review_ready")

    let action = app.buttons["Review outcome"]
    XCTAssertTrue(action.waitForExistence(timeout: 3))
    action.tap()

    XCTAssertTrue(app.tabBars.buttons["Review"].isSelected)
    XCTAssertTrue(app.navigationBars["Review"].exists)
  }

  @MainActor
  func testFlagshipTabsExposeTheirPrimaryJobs() {
    let app = launch("review_ready")

    XCTAssertTrue(app.navigationBars["Now"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.descendants(matching: .any)["now.session.forge_mobile_review"].exists)
    XCTAssertTrue(app.buttons["Review outcome"].exists)
    keepScreenshot(app, named: "01-now-review-ready")

    app.tabBars.buttons["Forge"].tap()
    XCTAssertTrue(app.navigationBars["Forge"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.textFields["forge.intent"].exists)
    XCTAssertTrue(app.buttons["forge.start-build"].exists)
    keepScreenshot(app, named: "02-forge-start-build")

    app.tabBars.buttons["Review"].tap()
    XCTAssertTrue(app.navigationBars["Review"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.descendants(matching: .any)["review.digest.review_reconnect"].exists)
    keepScreenshot(app, named: "03-review-digest")

    app.tabBars.buttons["You"].tap()
    XCTAssertTrue(app.navigationBars["You"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.descendants(matching: .any)["you.list"].exists)
    keepScreenshot(app, named: "04-you")

    app.tabBars.buttons["Now"].tap()
    let session = app.descendants(matching: .any)["now.session.forge_mobile_review"]
    XCTAssertTrue(session.waitForExistence(timeout: 3))
    session.tap()
    XCTAssertTrue(app.descendants(matching: .any)["session.timeline"].waitForExistence(timeout: 3))
    keepScreenshot(app, named: "05-session-timeline")
  }

  @MainActor
  func testReviewFlowExposesFocusedDiffAndGuardedDecisions() {
    let app = launch("review_ready")
    app.tabBars.buttons["Review"].tap()

    let focusedDiff = app.buttons.matching(
      NSPredicate(format: "label == %@", "Review 11 changed files")
    ).firstMatch
    XCTAssertTrue(focusedDiff.waitForExistence(timeout: 3))
    reveal(focusedDiff, in: app)
    focusedDiff.tap()
    XCTAssertTrue(app.navigationBars["Focused diff"].waitForExistence(timeout: 3))
    XCTAssertFalse(app.tabBars.firstMatch.exists)

    app.navigationBars.buttons.firstMatch.tap()
    let requestChanges = app.buttons.matching(
      NSPredicate(format: "label == %@", "Request changes")
    ).firstMatch
    reveal(requestChanges, in: app)
    requestChanges.tap()
    XCTAssertTrue(app.navigationBars["Request changes"].waitForExistence(timeout: 3))
    let sendChanges = app.buttons["review.send-changes"]
    XCTAssertFalse(sendChanges.isEnabled)
    app.textFields["review.change-request"].tap()
    app.textFields["review.change-request"].typeText("Keep the replay receipt visible.")
    XCTAssertTrue(sendChanges.isEnabled)
    app.navigationBars.buttons["Cancel"].tap()

    let approve = app.buttons.matching(
      NSPredicate(format: "label == %@", "Approve and open PR")
    ).firstMatch
    reveal(approve, in: app)
    approve.tap()
    XCTAssertTrue(app.navigationBars["Open pull request"].waitForExistence(timeout: 3))
    let confirm = app.buttons["review.approve-open"]
    XCTAssertFalse(confirm.isEnabled)
    app.textFields["review.ship-reason"].tap()
    app.textFields["review.ship-reason"].typeText("Proof matches the intended outcome.")
    XCTAssertTrue(confirm.isEnabled)
  }

  @MainActor
  func testOfflineBuildBecomesAnExplicitLocalDraft() {
    let app = launch("offline")
    app.tabBars.buttons["Forge"].tap()

    let brief = app.textFields["forge.intent"]
    XCTAssertTrue(brief.waitForExistence(timeout: 3))
    brief.tap()
    brief.typeText("Improve the mobile review summary")
    let repository = app.textFields["forge.repository"]
    repository.tap()
    repository.typeText("mdx-native")
    app.toolbars.buttons["Done"].tap()

    let startBuild = app.buttons["forge.start-build"]
    reveal(startBuild, in: app)
    XCTAssertTrue(startBuild.isEnabled)
    startBuild.tap()
    XCTAssertTrue(app.descendants(matching: .any)["forge.feedback"].waitForExistence(timeout: 3))

    app.tabBars.buttons["You"].tap()
    XCTAssertTrue(app.tabBars.buttons["You"].isSelected)
    let draft = app.staticTexts["Improve the mobile review summary"]
    reveal(draft, in: app)
    XCTAssertTrue(draft.waitForExistence(timeout: 3))
    let send = app.buttons.matching(NSPredicate(format: "label == %@", "Send")).firstMatch
    XCTAssertFalse(send.isEnabled)
    XCTAssertTrue(
      app.buttons.matching(NSPredicate(format: "label == %@", "Delete")).firstMatch.exists)
  }

  @MainActor
  func testNeedsYouRoutesToFocusedSessionWithoutPrimaryTabs() {
    let app = launch("needs_you")
    let reviewOptions = app.buttons["Review options"]
    XCTAssertTrue(reviewOptions.waitForExistence(timeout: 3))
    reviewOptions.tap()

    XCTAssertTrue(app.descendants(matching: .any)["session.timeline"].waitForExistence(timeout: 3))
    XCTAssertFalse(app.tabBars.firstMatch.exists)
  }

  @MainActor
  func testCriticalObjectsExposeUsefulAccessibilitySemantics() {
    let app = launch("review_ready")
    let session = app.descendants(matching: .any)["now.session.forge_mobile_review"]
    XCTAssertTrue(session.waitForExistence(timeout: 3))
    XCTAssertTrue(session.label.contains("mdx-native"))
    XCTAssertTrue(session.label.contains("Ready to review"))
    XCTAssertTrue(session.label.contains("Darren's MacBook Pro"))

    app.tabBars.buttons["Forge"].tap()
    let pairedMac = app.buttons["forge.target.paired_host"]
    XCTAssertTrue(pairedMac.waitForExistence(timeout: 3))
    XCTAssertTrue(pairedMac.label.contains("Your Mac"))
    XCTAssertTrue(pairedMac.label.contains("paired Mac"))
    XCTAssertTrue(pairedMac.isSelected)

    app.tabBars.buttons["Review"].tap()
    XCTAssertTrue(app.buttons["review.request-changes"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.buttons["review.approve-pr"].exists)
  }

  @MainActor
  func testPortraitOrientationLockKeepsReviewActionReachable() {
    let app = launch("review_ready")
    XCUIDevice.shared.orientation = .landscapeLeft
    defer { XCUIDevice.shared.orientation = .portrait }

    XCTAssertTrue(app.tabBars.buttons["Now"].waitForExistence(timeout: 3))
    XCTAssertGreaterThan(app.frame.height, app.frame.width)
    let review = app.buttons["Review outcome"]
    reveal(review, in: app)
    XCTAssertTrue(review.isHittable)
    keepScreenshot(app, named: "portrait-locked-review-action")
  }

  @MainActor
  func testYouRoutesExposeTrustCloudSupportAndDataControls() {
    let app = launch("review_ready")
    app.tabBars.buttons["You"].tap()

    app.buttons["MDx Cloud setup"].tap()
    XCTAssertTrue(app.navigationBars["MDx Cloud"].waitForExistence(timeout: 3))
    XCTAssertFalse(app.tabBars.firstMatch.exists)
    app.navigationBars.buttons.firstMatch.tap()

    app.buttons["Trusted devices"].tap()
    XCTAssertTrue(app.navigationBars["Trusted devices"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.buttons["trust.pair-mac"].exists)
    XCTAssertFalse(app.tabBars.firstMatch.exists)
    app.buttons["trust.pair-mac"].tap()
    XCTAssertTrue(app.navigationBars["Pair your Mac"].waitForExistence(timeout: 3))
    XCTAssertTrue(app.buttons["pairing.paste-code"].exists)
    app.navigationBars.buttons["Cancel"].tap()
    app.navigationBars.buttons.firstMatch.tap()

    let diagnostics = app.buttons["Connection diagnostics"]
    reveal(diagnostics, in: app)
    diagnostics.tap()
    XCTAssertTrue(app.navigationBars["Diagnostics"].waitForExistence(timeout: 3))
    XCTAssertFalse(app.tabBars.firstMatch.exists)
    app.navigationBars.buttons.firstMatch.tap()

    let privacy = app.buttons["Privacy and retention"]
    reveal(privacy, in: app)
    privacy.tap()
    XCTAssertTrue(app.navigationBars["Privacy"].waitForExistence(timeout: 3))
    XCTAssertFalse(app.tabBars.firstMatch.exists)
    app.navigationBars.buttons.firstMatch.tap()

    let deleteLocal = app.buttons["Delete local mobile data"]
    reveal(deleteLocal, in: app)
    deleteLocal.tap()
    XCTAssertTrue(app.buttons["Delete local data"].waitForExistence(timeout: 3))
    app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.1)).tap()

    let deleteAccount = app.buttons["Request account deletion"]
    reveal(deleteAccount, in: app)
    deleteAccount.tap()
    XCTAssertTrue(app.buttons["Confirm with Face ID"].waitForExistence(timeout: 3))
  }
}
