import Foundation

/// Batches high-frequency stream elements (Twin tokens, Forge SSE events)
/// into ~50ms flushes so the UI invalidates a bounded number of times per
/// second instead of once per token. Elements are never dropped: a flush is
/// scheduled the moment the first element lands, and `drain()` forces out
/// anything still buffered (call it at stream end and before terminal
/// mutations so ordering is preserved).
@MainActor
final class StreamCoalescer<Element> {
  private var buffer: [Element] = []
  private var flushTask: Task<Void, Never>?
  private let intervalNs: UInt64
  private let apply: ([Element]) -> Void

  init(intervalMs: UInt64 = 50, apply: @escaping ([Element]) -> Void) {
    self.intervalNs = intervalMs * 1_000_000
    self.apply = apply
  }

  func add(_ element: Element) {
    buffer.append(element)
    guard flushTask == nil else { return }
    flushTask = Task { [weak self, intervalNs] in
      try? await Task.sleep(nanoseconds: intervalNs)
      guard !Task.isCancelled else { return }
      self?.drain()
    }
  }

  func drain() {
    flushTask?.cancel()
    flushTask = nil
    guard !buffer.isEmpty else { return }
    let items = buffer
    buffer = []
    apply(items)
  }
}
