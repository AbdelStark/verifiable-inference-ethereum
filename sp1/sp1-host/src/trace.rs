//! The share of an SP1 profiler trace's samples spent in one function (`sp1-host trace-share`): the
//! Gecko profile that sp1-sdk's `profiling` executor writes to `TRACE_FILE`, one sample every
//! `TRACE_SAMPLE_RATE` cycles.

use serde_json::Value;

/// `(samples, matched)`: every sample of the trace's first thread, and those whose call stack holds
/// a frame whose name contains `function`.
pub fn function_samples(trace: &Value, function: &str) -> Result<(u64, u64), String> {
    let thread = &trace["threads"][0];
    let table = |name: &str| -> Result<(&Vec<Value>, &Value), String> {
        let t = &thread[name];
        Ok((
            t["data"].as_array().ok_or(format!("no {name}.data"))?,
            &t["schema"],
        ))
    };
    let index = |schema: &Value, field: &str| {
        schema[field]
            .as_u64()
            .map(|i| i as usize)
            .ok_or(format!("no schema field {field}"))
    };
    let strings = thread["stringTable"].as_array().ok_or("no stringTable")?;
    let (frames, frame_schema) = table("frameTable")?;
    let (stacks, stack_schema) = table("stackTable")?;
    let (samples, sample_schema) = table("samples")?;
    let (location, frame, prefix, stack) = (
        index(frame_schema, "location")?,
        index(stack_schema, "frame")?,
        index(stack_schema, "prefix")?,
        index(sample_schema, "stack")?,
    );
    let frame_matches: Vec<bool> = frames
        .iter()
        .map(|f| {
            let name = f[location]
                .as_u64()
                .and_then(|i| strings[i as usize].as_str());
            name.is_some_and(|n| n.contains(function))
        })
        .collect();
    // A stack's prefix always comes before it in the table, so one pass resolves every stack.
    let mut stack_matches: Vec<bool> = Vec::with_capacity(stacks.len());
    for s in stacks {
        let own = frame_matches[s[frame].as_u64().ok_or("stack without frame")? as usize];
        let parent = s[prefix]
            .as_u64()
            .is_some_and(|p| stack_matches[p as usize]);
        stack_matches.push(own || parent);
    }
    let matched = samples
        .iter()
        .filter(|s| s[stack].as_u64().is_some_and(|i| stack_matches[i as usize]))
        .count();
    Ok((samples.len() as u64, matched as u64))
}
