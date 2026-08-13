use mdx_core::LoopRunReport;

pub(crate) fn render_report(report: &LoopRunReport) -> String {
    format!(
        "run_id: {}\nloop_id: {}\nstatus: {}\nscore: {}\ncredential_status: {}\nreceipt_count: {}\n{}\n",
        report.run_id,
        report.loop_id,
        report.status,
        report.score,
        report.credential_status,
        report.receipts.len(),
        report.concierge_answer
    )
}
