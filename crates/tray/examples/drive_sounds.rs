//! Writes the drive's sounds as WAV files, for listening to.
//! `drive_sounds <out dir>`
use tray::sound::Sound;

fn wav(path: &std::path::Path, rate: u32, samples: &[i16]) {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).unwrap();
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    let rate = 44_100;
    let mut all = Vec::new();
    for (name, sound) in [
        ("insert", Sound::PullIn),
        ("spin-up", Sound::SpinUp),
        ("eject", Sound::PushOut),
    ] {
        let s = sound.render(rate);
        wav(&dir.join(format!("disc-{name}.wav")), rate, &s);
        all.extend_from_slice(&s);
        all.extend(std::iter::repeat_n(0i16, rate as usize / 2 * 2));
    }
    wav(&dir.join("disc-all.wav"), rate, &all);
}
