//! End-to-end voice memory check with the real models and macOS text-to-speech voices
//! (synthetic, no real recordings). Run: `cargo test --release --test voice_memory -- --ignored --nocapture`.

#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;

use pushtalks_lib::asr::diarize::Diarizer;
use pushtalks_lib::meeting::pipeline;
use pushtalks_lib::voices::{merge, Voice};

fn say(voice: &str, text: &str, dir: &Path, name: &str) -> Vec<i16> {
    let aiff = dir.join(format!("{name}.aiff"));
    let wav = dir.join(format!("{name}.wav"));
    assert!(Command::new("say")
        .args(["-v", voice, "-o"])
        .arg(&aiff)
        .arg(text)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
        .arg(&aiff)
        .arg(&wav)
        .status()
        .unwrap()
        .success());
    hound::WavReader::open(&wav)
        .unwrap()
        .samples::<i16>()
        .map(|s| s.unwrap())
        .collect()
}

fn write(path: &Path, samples: &[i16]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for s in samples {
        w.write_sample(*s).unwrap();
    }
    w.finalize().unwrap();
}

/// A meeting: mic has "me" lines, system has the remote lines, in turn with short gaps.
fn meeting(dir: &Path, lines: &[(&str, bool, &str)]) {
    std::fs::create_dir_all(dir).unwrap();
    let (mut mic, mut sys) = (vec![0i16; 16_000], vec![0i16; 16_000]);
    for (i, (voice, remote, text)) in lines.iter().enumerate() {
        let a = say(voice, text, dir, &format!("l{i}"));
        let (target, other) = if *remote {
            (&mut sys, &mut mic)
        } else {
            (&mut mic, &mut sys)
        };
        target.extend_from_slice(&a);
        other.extend(std::iter::repeat_n(0, a.len()));
        for t in [&mut mic, &mut sys] {
            t.extend(std::iter::repeat_n(0, 12_000));
        }
    }
    write(&dir.join("mic.wav"), &mic);
    write(&dir.join("system.wav"), &sys);
}

#[test]
#[ignore]
fn names_known_voices_and_numbers_strangers() {
    let models = pushtalks_lib::models::models_dir();
    let loaded =
        pushtalks_lib::pipeline::load(&models, pushtalks_lib::pipeline::default_threads()).unwrap();
    let d = Diarizer::load(&models, 4)
        .unwrap()
        .expect("diarization models installed");
    let root = std::env::temp_dir().join(format!("pt-voices-{}", std::process::id()));

    // Meeting A: Daniel and Karen speak; the user names them afterwards.
    let a = root.join("a");
    meeting(&a, &[
        ("Samantha", false, "Thanks for joining, let's go through the quarterly numbers together."),
        ("Daniel", true, "Sure. Search performance was strong this month and pacing is ahead of plan, mostly thanks to the new keyword strategy we launched in September."),
        ("Karen", true, "Social was slower. The creative refresh went live late, so we only had two weeks of data, but engagement since then has been encouraging."),
        ("Daniel", true, "I can put together a revised budget proposal that moves some spend from search into social for the next flight."),
        ("Karen", true, "That works for me. Let's also test two new video concepts so we have a fair comparison by the end of the month."),
    ]);
    let pa = pipeline::process(&loaded, Some(&d), &a, &[], Some(&[]), |_| {}).unwrap();
    let labels: Vec<&str> = pa.voiceprints.iter().map(|(l, _)| l.as_str()).collect();
    println!("meeting A speakers: {labels:?}");
    let print = |l: &str| {
        pa.voiceprints
            .iter()
            .find(|(x, _)| x == l)
            .map(|(_, e)| e.clone())
            .unwrap()
    };
    let voices = vec![
        Voice {
            id: 1,
            name: "Daniel".into(),
            embedding: merge(&[], 0, &print("Speaker 1")),
            samples: 1,
        },
        Voice {
            id: 2,
            name: "Karen".into(),
            embedding: merge(&[], 0, &print("Speaker 2")),
            samples: 1,
        },
    ];

    // Meeting B: new sentences from Daniel and Karen, plus Fred whom PushTalks has never heard.
    let b = root.join("b");
    meeting(&b, &[
        ("Samantha", false, "Okay, quick follow up on last week's plan."),
        ("Karen", true, "The two video concepts are ready, and the first one is already outperforming our old creative on click-through rate."),
        ("Fred", true, "Hi everyone, I'm joining from the analytics side. I can set up the reporting dashboard so we track both concepts daily."),
        ("Daniel", true, "Great. I've sent the revised budget, and the shift from search into social starts on Monday morning."),
        ("Fred", true, "I'll make sure the dashboard is live before then, and I'll share the link with the whole team."),
    ]);
    let pb = pipeline::process(&loaded, Some(&d), &b, &[], Some(&voices), |_| {}).unwrap();
    for (l, e) in &pb.voiceprints {
        let sims: Vec<String> = voices
            .iter()
            .map(|v| {
                format!(
                    "{} {:.2}",
                    v.name,
                    pushtalks_lib::voices::cosine(e, &v.embedding)
                )
            })
            .collect();
        println!("meeting B {l}: {}", sims.join(", "));
    }
    let speakers: Vec<String> = pb
        .segments
        .iter()
        .map(|s| format!("{}: {}", s.speaker, &s.text[..s.text.len().min(30)]))
        .collect();
    println!("{}", speakers.join("\n"));
    let who = |needle: &str| {
        pb.segments
            .iter()
            .find(|s| s.text.contains(needle))
            .map(|s| s.speaker.clone())
            .unwrap()
    };
    assert_eq!(who("video concepts"), "Karen");
    assert_eq!(who("revised budget"), "Daniel");
    assert!(
        who("analytics").starts_with("Speaker"),
        "a stranger keeps a number"
    );
    std::fs::remove_dir_all(root).unwrap();
}
