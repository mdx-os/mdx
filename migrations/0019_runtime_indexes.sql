CREATE INDEX IF NOT EXISTS ledger_entries_tenant_kind_created_idx
  ON ledger_entries (tenant_id, kind, created_at DESC);

CREATE INDEX IF NOT EXISTS ledger_entries_tenant_actor_created_idx
  ON ledger_entries (tenant_id, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS ledger_entries_trace_created_idx
  ON ledger_entries (trace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS ledger_entries_policy_decision_idx
  ON ledger_entries (policy_decision_id)
  WHERE policy_decision_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS outbox_events_tenant_topic_created_idx
  ON outbox_events (tenant_id, topic, created_at DESC);

CREATE INDEX IF NOT EXISTS outbox_events_tenant_pending_idx
  ON outbox_events (tenant_id, created_at)
  WHERE delivered_at IS NULL;

CREATE INDEX IF NOT EXISTS outbox_events_source_receipt_idx
  ON outbox_events (source_receipt_id);

CREATE INDEX IF NOT EXISTS message_threads_channel_created_idx
  ON message_threads (tenant_id, channel_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_thread_messages_thread_created_desc_idx
  ON message_thread_messages (tenant_id, thread_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_thread_messages_actor_created_idx
  ON message_thread_messages (tenant_id, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_thread_messages_source_receipt_idx
  ON message_thread_messages (source_receipt_id);

CREATE INDEX IF NOT EXISTS message_envelopes_channel_created_idx
  ON message_envelopes (tenant_id, channel_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_envelopes_state_created_idx
  ON message_envelopes (tenant_id, envelope_state, created_at DESC);

CREATE INDEX IF NOT EXISTS message_envelopes_source_receipt_idx
  ON message_envelopes (source_receipt_id);

CREATE INDEX IF NOT EXISTS message_delivery_replay_batches_channel_created_idx
  ON message_delivery_replay_batches (tenant_id, channel_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_delivery_replay_batches_preflight_idx
  ON message_delivery_replay_batches (tenant_id, realtime_preflight_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_realtime_cutover_preflights_channel_created_idx
  ON message_realtime_cutover_preflights (tenant_id, channel_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_subscription_isolation_checks_preflight_idx
  ON message_subscription_isolation_checks (tenant_id, realtime_preflight_id, created_at DESC);

CREATE INDEX IF NOT EXISTS message_service_role_fanout_refusals_tenant_preflight_idx
  ON message_service_role_fanout_refusals (tenant_id, realtime_preflight_id, created_at DESC);
