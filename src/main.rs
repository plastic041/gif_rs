use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fmt, fs,
    io::{self, stdin, Write},
    process::Command,
};

#[derive(Debug)]
struct Duration {
    h: u32,
    m: u32,
    s: u32,
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.h, self.m, self.s)
    }
}

impl Duration {
    fn from_str(input: &str) -> Result<Self> {
        let parts: Vec<&str> = input.split(':').collect();
        let duration = match parts.len() {
            1 => Self {
                h: 0,
                m: 0,
                s: parts[0].parse().context("Failed to parse seconds")?,
            },
            2 => Self {
                h: 0,
                m: parts[0].parse().context("Failed to parse minutes")?,
                s: parts[1].parse().context("Failed to parse seconds")?,
            },
            3 => Self {
                h: parts[0].parse().context("Failed to parse hours")?,
                m: parts[1].parse().context("Failed to parse minutes")?,
                s: parts[2].parse().context("Failed to parse seconds")?,
            },
            _ => anyhow::bail!("Invalid duration format"),
        };
        Ok(duration)
    }

    fn from_seconds(seconds: f64) -> Self {
        let total_seconds = seconds.floor() as u32;
        Self {
            h: total_seconds / 3600,
            m: (total_seconds % 3600) / 60,
            s: total_seconds % 60,
        }
    }

    fn to_seconds(&self) -> u32 {
        self.h * 3600 * self.m * 60 + self.s
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = stdin();

    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Usage: <program> <input_file>");
        return Ok(());
    }

    let input_file = &args[1];
    let filename = input_file.split('.').next().unwrap_or("output");

    let (resolution, duration, fps) = get_file_info(input_file)?;

    println!("Input file: {}", input_file);
    println!("Resolution: {}x{}", resolution.width, resolution.height);
    println!("Duration: {}", duration);
    println!("FPS: {}", fps);

    let start_time = prompt_user("시작 시간 (hh:mm:ss, Enter=00:00:00): ");
    let start_time = if start_time.is_empty() {
        Duration::from_seconds(0.0)
    } else {
        Duration::from_str(&start_time)?
    };

    let end_time = prompt_user("끝 시간 (hh:mm:ss, Enter=끝까지): ");
    let end_time = if end_time.is_empty() {
        duration
    } else {
        Duration::from_str(&end_time)?
    };

    println!("FPS (Enter={}): ", fps);
    let mut fps_input = String::new();
    stdin.read_line(&mut fps_input)?;

    let framecount = (end_time.to_seconds() as f32 - start_time.to_seconds() as f32) * fps;

    println!("가로 픽셀 크기 (Enter=원본크기): ");
    let mut width_input = String::new();
    stdin.read_line(&mut width_input)?;

    let output_file = format!("{}.gif", filename);

    let temp_folder = "output_frames";

    let _ = fs::create_dir(temp_folder);

    let _output_thumbnails = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostats",
            "-v",
            "warning",
            "-ss",
            &start_time.to_string(),
            "-to",
            &end_time.to_string(),
            "-i",
            input_file,
            "-fps_mode",
            "vfr",
            "-lavfi",
            &format!(r"fps={},scale=600:-1:flags=lanczos", fps),
            "-q:v",
            "15",
            "-y",
            &format!("./{}/%04d.jpg", temp_folder),
        ])
        .output()?;
    check_command_success(&_output_thumbnails)?;

    println!("시작 프레임 수");
    let mut start_frame_input = String::new();
    stdin.read_line(&mut start_frame_input)?;
    let start_frame = start_frame_input.trim();
    let start_frame = if start_frame.is_empty() {
        1
    } else {
        start_frame.parse().expect("Start frame is not number")
    };

    println!("끝 프레임 수 (Enter={})", framecount);
    let mut end_frame_input = String::new();
    stdin.read_line(&mut end_frame_input)?;
    let end_frame = end_frame_input.trim();
    let end_frame = if end_frame.is_empty() {
        framecount
    } else {
        end_frame.parse().expect("Start frame is not number")
    };

    let output_palette = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostats",
            "-v",
            "warning",
            "-ss",
            &start_time.to_string(),
            "-to",
            &end_time.to_string(),
            "-i",
            input_file,
            "-fps_mode",
            "vfr",
            "-lavfi",
            &format!(
                "fps={},trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS,scale={}:-1:flags=lanczos,palettegen=stats_mode=diff",
                fps, start_frame, end_frame, resolution.width
            ),
            "-y",
            "palette.png",
        ])
        .output()?;
    check_command_success(&output_palette)?;

    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostats",
            "-v",
            "warning",
            "-ss",
            &start_time.to_string(),
            "-to",
            &end_time.to_string(),
            "-i",
            input_file,
            "-i",
            "palette.png",
            "-fps_mode",
            "vfr",
            "-lavfi",
            &format!(
                "fps={},trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS,scale={}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3",
                fps, start_frame, end_frame, resolution.width
            ),
            "-y",
            &output_file,
        ])
        .output()?;
    check_command_success(&output)?;

    fs::remove_file("palette.png")?; // Clean up palette file
    fs::remove_dir_all(temp_folder)?;

    println!("{} 파일 생성 완료!", output_file);

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct ProbeInfo {
    streams: Vec<StreamInfo>,
    format: FormatInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct StreamInfo {
    width: Option<u32>,
    height: Option<u32>,
    codec_type: String,
    r_frame_rate: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FormatInfo {
    duration: String,
}

#[derive(Debug)]
struct Resolution {
    width: u32,
    height: u32,
}

fn get_file_info(filename: &str) -> Result<(Resolution, Duration, f32)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            filename,
        ])
        .output()
        .context("Failed to execute ffprobe")?;

    let info: ProbeInfo =
        serde_json::from_slice(&output.stdout).context("Failed to parse ffprobe output")?;
    let stream = info
        .streams
        .iter()
        .find(|s| s.codec_type == "video")
        .context("No video stream found")?;

    let width = stream.width.context("Width not found")?;
    let height = stream.height.context("Height not found")?;
    let resolution = Resolution { width, height };

    let duration = Duration::from_seconds(
        info.format
            .duration
            .parse::<f64>()
            .context("Invalid duration")?,
    );

    let fps_parts: Vec<&str> = stream.r_frame_rate.split('/').collect();
    let fps = if fps_parts.len() == 2 {
        let numerator = fps_parts[0]
            .parse::<f32>()
            .context("Invalid FPS numerator")?;
        let denominator = fps_parts[1]
            .parse::<f32>()
            .context("Invalid FPS denominator")?;
        numerator / denominator
    } else {
        anyhow::bail!("Invalid FPS format")
    };

    Ok((resolution, duration, fps))
}

fn prompt_user(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn check_command_success(output: &std::process::Output) -> Result<()> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Command failed: {}", stderr);
    }
    Ok(())
}
