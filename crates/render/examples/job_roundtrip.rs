//! A maintainer check, for development only: loads a dumped render job with
//! `Job::from_json`, says what it holds, and says whether dumping it again
//! gives the same text. Exit 0 when it does, 1 when the job is rejected or
//! the text differs, 2 when called wrongly.

use render::Job;

fn main() {
    let Some(file) = std::env::args().nth(1) else {
        eprintln!("usage: job_roundtrip <job.json>");
        std::process::exit(2);
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("{file}: {error}");
            std::process::exit(2);
        }
    };
    let job = match Job::from_json(&text) {
        Ok(job) => job,
        Err(message) => {
            println!("rejected: {message}");
            std::process::exit(1);
        }
    };
    println!("loaded");
    println!("  run_id   {} characters", job.run_id.len());
    println!("  project  {}", job.project.display());
    println!("  entry    {}", job.entry);
    println!("  output   {}", job.output.display());
    println!("  fps      {}", job.fps);
    println!("  frames   {}", job.frames);
    println!(
        "  manifest {} {}x{}, {} fps written, {} clips, {} tweens",
        job.manifest.name,
        job.manifest.width,
        job.manifest.height,
        job.manifest.fps,
        job.manifest.clips.len(),
        job.manifest.tweens.len()
    );
    println!(
        "  format   {} {} {} quality {} preset {}",
        job.format.container, job.format.codec, job.format.pixel_format, job.format.quality, job.format.preset
    );
    let again = job.to_json();
    let same_text = again == text;
    let same_job = Job::from_json(&again).as_ref() == Ok(&job);
    println!(
        "dumped again: {}",
        if same_text {
            format!("the same {} bytes", again.len())
        } else {
            format!("DIFFERENT ({} bytes against {})", again.len(), text.len())
        }
    );
    println!(
        "loaded again from that dump: {}",
        if same_job {
            "equal to the first load in every field"
        } else {
            "NOT equal"
        }
    );
    std::process::exit(if same_text && same_job { 0 } else { 1 });
}
