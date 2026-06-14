//! `builtin.job-progress` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    dom::set_text,
    host_state::{DocumentState, JobsState, section},
    runtime::host_section,
};

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    // BL-142: 個別 getter ではなくセクション JSON を 1 回取得し型付き DTO へ落とす。
    let jobs = host_section::<JobsState>(section::JOBS).unwrap_or(JobsState {
        active: 0,
        queued: 0,
    });
    let title = host_section::<DocumentState>(section::DOCUMENT)
        .map(|document| document.title)
        .unwrap_or_default();

    set_text("#active", &jobs.active.to_string());
    set_text("#queued", &jobs.queued.to_string());
    // BL-094: host state は生データのみ。status 文字列の整形はパネル側で行う。
    set_text("#status", &format_status(jobs.active, &title));
}

/// アクティブジョブ数と作品タイトルから status 文字列を整形する (BL-094)。
fn format_status(active: i64, work_title: &str) -> String {
    if active <= 0 {
        format!("idle / work={work_title}")
    } else {
        format!("{active} job(s) running")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
    });

    #[test]
    fn format_status_idle_includes_work_title() {
        assert_eq!(format_status(0, "untitled"), "idle / work=untitled");
    }

    #[test]
    fn format_status_running_counts_jobs() {
        assert_eq!(format_status(3, "untitled"), "3 job(s) running");
    }
}
