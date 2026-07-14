use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static SAVE_REQUESTED: AtomicBool = AtomicBool::new(false);

const MAX_FRAME_HISTORY: usize = 10_000;
const MAX_SCOPE_HISTORY: usize = 1_024;
const LIVE_WINDOW: usize = 120;
const STUTTER_THRESHOLD_MS: f64 = 50.0;

#[derive(Clone, Debug)]
struct Entry {
    total_secs: f64,
    count: u64,
    min_secs: f64,
    max_secs: f64,
    samples_ms: VecDeque<f64>,
}

#[derive(Clone, Debug)]
struct FrameSample {
    number: u64,
    time_ms: f64,
    scopes_ms: HashMap<&'static str, f64>,
    gauges: HashMap<&'static str, f64>,
}

#[derive(Clone, Debug)]
pub struct ProfilerSnapshot {
    pub frames: usize,
    pub latest_frame_ms: f64,
    pub average_frame_ms: f64,
    pub p95_frame_ms: f64,
    pub recent_stutters: usize,
    pub top_scopes: Vec<(&'static str, f64)>,
}

thread_local! {
    static PROFILER: RefCell<Profiler> = RefCell::new(Profiler::new());
}

struct Profiler {
    data: HashMap<&'static str, Entry>,
    stack: Vec<(&'static str, Instant)>,
    enabled: bool,
    frame_count: u64,
    frames: VecDeque<FrameSample>,
    current_frame_scopes: HashMap<&'static str, f64>,
    current_gauges: HashMap<&'static str, f64>,
}

impl Profiler {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
            stack: Vec::new(),
            enabled: false,
            frame_count: 0,
            frames: VecDeque::new(),
            current_frame_scopes: HashMap::new(),
            current_gauges: HashMap::new(),
        }
    }

    fn begin(&mut self, label: &'static str) {
        if self.enabled {
            self.stack.push((label, Instant::now()));
        }
    }

    fn end(&mut self, label: &'static str) {
        if !self.enabled {
            return;
        }

        let Some((actual_label, start)) = self.stack.pop() else {
            log::error!("profiler scope ended without a matching begin: {label}");
            self.stack.clear();
            return;
        };
        if actual_label != label {
            log::error!("profiler scope mismatch: expected {label}, got {actual_label}");
            self.stack.clear();
            return;
        }

        let elapsed_secs = start.elapsed().as_secs_f64();
        let elapsed_ms = elapsed_secs * 1000.0;
        let entry = self.data.entry(label).or_insert_with(|| Entry {
            total_secs: 0.0,
            count: 0,
            min_secs: f64::MAX,
            max_secs: 0.0,
            samples_ms: VecDeque::new(),
        });
        entry.total_secs += elapsed_secs;
        entry.count += 1;
        entry.min_secs = entry.min_secs.min(elapsed_secs);
        entry.max_secs = entry.max_secs.max(elapsed_secs);
        push_bounded(&mut entry.samples_ms, elapsed_ms, MAX_SCOPE_HISTORY);

        if label == "frame" {
            self.frame_count += 1;
            let sample = FrameSample {
                number: self.frame_count,
                time_ms: elapsed_ms,
                scopes_ms: std::mem::take(&mut self.current_frame_scopes),
                gauges: std::mem::take(&mut self.current_gauges),
            };
            push_bounded(&mut self.frames, sample, MAX_FRAME_HISTORY);
            if self.frame_count % 300 == 0 {
                SAVE_REQUESTED.store(true, Ordering::Relaxed);
            }
        } else {
            *self.current_frame_scopes.entry(label).or_insert(0.0) += elapsed_ms;
        }
    }

    fn set_gauge(&mut self, label: &'static str, value: f64) {
        if self.enabled {
            self.current_gauges.insert(label, value);
        }
    }
}

fn push_bounded<T>(values: &mut VecDeque<T>, value: T, max_len: usize) {
    if values.len() == max_len {
        values.pop_front();
    }
    values.push_back(value);
}

fn percentile(values: impl IntoIterator<Item = f64>, percentile: f64) -> Option<f64> {
    let mut sorted: Vec<f64> = values.into_iter().collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    Some(sorted[index])
}

pub struct Scope {
    label: &'static str,
}

impl Scope {
    pub fn new(label: &'static str) -> Self {
        PROFILER.with(|p| p.borrow_mut().begin(label));
        Self { label }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        PROFILER.with(|p| p.borrow_mut().end(self.label));
    }
}

pub fn set_enabled(enabled: bool) {
    PROFILER.with(|p| p.borrow_mut().enabled = enabled);
}

pub fn is_enabled() -> bool {
    PROFILER.with(|p| p.borrow().enabled)
}

pub fn begin(label: &'static str) {
    PROFILER.with(|p| p.borrow_mut().begin(label));
}

pub fn end(label: &'static str) {
    PROFILER.with(|p| p.borrow_mut().end(label));
}

pub fn set_gauge(label: &'static str, value: f64) {
    PROFILER.with(|p| p.borrow_mut().set_gauge(label, value));
}

pub fn snapshot() -> Option<ProfilerSnapshot> {
    PROFILER.with(|p| {
        let p = p.borrow();
        let latest = p.frames.back()?;
        let window = p.frames.len().min(LIVE_WINDOW);
        let recent = p.frames.iter().rev().take(window);
        let frame_times: Vec<f64> = recent.map(|frame| frame.time_ms).collect();
        let average_frame_ms = frame_times.iter().sum::<f64>() / frame_times.len() as f64;
        let p95_frame_ms = percentile(frame_times.iter().copied(), 0.95)?;
        let recent_stutters = frame_times
            .iter()
            .filter(|time| **time >= STUTTER_THRESHOLD_MS)
            .count();

        let mut top_scopes: Vec<_> = latest
            .scopes_ms
            .iter()
            .map(|(&label, &time)| (label, time))
            .collect();
        top_scopes.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        top_scopes.truncate(3);

        Some(ProfilerSnapshot {
            frames: p.frames.len(),
            latest_frame_ms: latest.time_ms,
            average_frame_ms,
            p95_frame_ms,
            recent_stutters,
            top_scopes,
        })
    })
}

pub fn new_frame() {
    if SAVE_REQUESTED.swap(false, Ordering::Relaxed) {
        save("/tmp/opencode/profiler_output.txt");
    }
}

pub fn reset() {
    PROFILER.with(|p| {
        let mut p = p.borrow_mut();
        p.data.clear();
        p.stack.clear();
        p.frame_count = 0;
        p.frames.clear();
        p.current_frame_scopes.clear();
        p.current_gauges.clear();
    });
}

fn timestamped_path(base: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Some(stem) = base.strip_suffix(".txt") {
        format!("{stem}_{ts}.txt")
    } else {
        format!("{base}_{ts}.txt")
    }
}

pub fn save(path: &str) -> String {
    let actual = timestamped_path(path);
    let ok = PROFILER.with(|p| {
        let p = p.borrow();
        if !p.enabled || p.data.is_empty() {
            return false;
        }

        let mut lines = vec![format!("=== Profiler dump: {actual} ==="), String::new()];
        let frame_total = p.data.get("frame").map_or(0.0, |entry| entry.total_secs);

        lines.push(String::from(
            "=== SCOPE SUMMARY (inclusive; nested scopes overlap) ===",
        ));
        lines.push(format!(
            "{:<32} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10} {:>8}",
            "Scope", "Count", "Total ms", "Avg ms", "P95 ms", "Min ms", "Max ms", "Frame %"
        ));
        lines.push("-".repeat(112));
        let mut scopes: Vec<_> = p.data.iter().collect();
        scopes.sort_unstable_by(|a, b| b.1.total_secs.total_cmp(&a.1.total_secs));
        for (label, entry) in scopes {
            let average_ms = entry.total_secs / entry.count as f64 * 1000.0;
            let p95_ms = percentile(entry.samples_ms.iter().copied(), 0.95).unwrap_or(0.0);
            let frame_percent = if frame_total > 0.0 {
                entry.total_secs / frame_total * 100.0
            } else {
                0.0
            };
            lines.push(format!(
                "{:<32} {:>8} {:>10.2} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>7.1}%",
                label,
                entry.count,
                entry.total_secs * 1000.0,
                average_ms,
                p95_ms,
                entry.min_secs * 1000.0,
                entry.max_secs * 1000.0,
                frame_percent,
            ));
        }

        append_frame_timing(&mut lines, &p.frames);
        append_workload_summary(&mut lines, &p.frames);
        append_slowest_frames(&mut lines, &p.frames);

        lines.push(String::new());
        lines.push(format!("Total frames captured: {}", p.frames.len()));
        lines.push(format!(
            "Total frame time sampled: {:.3}ms",
            frame_total * 1000.0
        ));
        fs::write(&actual, lines.join("\n")).is_ok()
    });
    if ok {
        eprintln!("Profiler data saved to {actual}");
    } else {
        eprintln!("Profiler: no data to save (was the profiler enabled?)");
    }
    actual
}

fn append_frame_timing(lines: &mut Vec<String>, frames: &VecDeque<FrameSample>) {
    lines.push(String::new());
    lines.push(String::from("=== FRAME TIMING ==="));
    if frames.is_empty() {
        lines.push(String::from("No completed frames captured."));
        return;
    }
    let times: Vec<f64> = frames.iter().map(|frame| frame.time_ms).collect();
    let average = times.iter().sum::<f64>() / times.len() as f64;
    let min = times.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let max = times.iter().copied().reduce(f64::max).unwrap_or(0.0);
    let p50 = percentile(times.iter().copied(), 0.50).unwrap_or(0.0);
    let p95 = percentile(times.iter().copied(), 0.95).unwrap_or(0.0);
    let p99 = percentile(times.iter().copied(), 0.99).unwrap_or(0.0);
    lines.push(format!("Frames recorded: {}", times.len()));
    lines.push(format!(
        "Frame time: avg={average:.2}ms min={min:.2}ms max={max:.2}ms"
    ));
    lines.push(format!(
        "Percentiles: P50={p50:.2}ms P95={p95:.2}ms P99={p99:.2}ms"
    ));
    lines.push(format!(
        "Effective FPS: avg={:.1} min={:.1}",
        1000.0 / average,
        1000.0 / max
    ));

    lines.push(String::new());
    lines.push(String::from("=== FRAME TIME HISTOGRAM ==="));
    for (label, low, high) in [
        ("<8ms     (>125fps)", 0.0, 8.0),
        ("8-16ms    (63-125fps)", 8.0, 16.0),
        ("16-33ms   (30-63fps)", 16.0, 33.0),
        ("33-50ms   (20-30fps)", 33.0, 50.0),
        ("50-100ms  (10-20fps)", 50.0, 100.0),
        ("100-200ms (5-10fps)", 100.0, 200.0),
        (">200ms    (<5fps)", 200.0, f64::MAX),
    ] {
        let count = times
            .iter()
            .filter(|time| **time >= low && **time < high)
            .count();
        let bar_len = (count as f64 / times.len() as f64 * 50.0).round() as usize;
        lines.push(format!(
            "  {label:<22} {count:>6} ({:>4.1}%) {}",
            count as f64 / times.len() as f64 * 100.0,
            "#".repeat(bar_len),
        ));
    }
}

fn append_workload_summary(lines: &mut Vec<String>, frames: &VecDeque<FrameSample>) {
    let mut values: HashMap<&str, Vec<f64>> = HashMap::new();
    for frame in frames {
        for (&label, &value) in &frame.gauges {
            values.entry(label).or_default().push(value);
        }
    }
    if values.is_empty() {
        return;
    }
    lines.push(String::new());
    lines.push(String::from("=== WORKLOAD (latest / avg / min / max) ==="));
    let mut labels: Vec<_> = values.keys().copied().collect();
    labels.sort_unstable();
    for label in labels {
        let samples = &values[label];
        let latest = samples.last().copied().unwrap_or(0.0);
        let average = samples.iter().sum::<f64>() / samples.len() as f64;
        let min = samples.iter().copied().reduce(f64::min).unwrap_or(0.0);
        let max = samples.iter().copied().reduce(f64::max).unwrap_or(0.0);
        lines.push(format!(
            "{label:<24} {latest:>8.0} / {average:>8.1} / {min:>8.0} / {max:>8.0}"
        ));
    }
}

fn append_slowest_frames(lines: &mut Vec<String>, frames: &VecDeque<FrameSample>) {
    lines.push(String::new());
    lines.push(String::from("=== TOP 10 SLOWEST FRAMES ==="));
    let mut slowest: Vec<_> = frames.iter().collect();
    slowest.sort_unstable_by(|a, b| b.time_ms.total_cmp(&a.time_ms));
    for frame in slowest.into_iter().take(10) {
        let mut scopes: Vec<_> = frame.scopes_ms.iter().collect();
        scopes.sort_unstable_by(|a, b| b.1.total_cmp(a.1));
        let scope_summary = scopes
            .into_iter()
            .take(3)
            .map(|(label, time)| format!("{label}={time:.2}ms"))
            .collect::<Vec<_>>()
            .join(", ");
        let flag = if frame.time_ms >= STUTTER_THRESHOLD_MS {
            " STUTTER"
        } else {
            ""
        };
        lines.push(format!(
            "Frame #{:>5}: {:>7.2}ms{flag}  {scope_summary}",
            frame.number, frame.time_ms
        ));
        if !frame.gauges.is_empty() {
            let mut gauges: Vec<_> = frame.gauges.iter().collect();
            gauges.sort_unstable_by(|a, b| a.0.cmp(b.0));
            let gauge_summary = gauges
                .into_iter()
                .map(|(label, value)| format!("{label}={value:.0}"))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("             workload: {gauge_summary}"));
        }
    }
    let stutters = frames
        .iter()
        .filter(|frame| frame.time_ms >= STUTTER_THRESHOLD_MS)
        .count();
    lines.push(format!(
        "Stutters (>={STUTTER_THRESHOLD_MS:.0}ms): {stutters} / {}",
        frames.len()
    ));
}
