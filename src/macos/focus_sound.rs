use objc2::{AnyThread, rc::Retained};
use objc2_app_kit::NSSound;
use objc2_foundation::NSData;
use shuidi::model::SoundTrack;

#[derive(Default)]
pub(super) struct FocusSound {
    track: Option<SoundTrack>,
    sound: Option<Retained<NSSound>>,
}

impl FocusSound {
    pub(super) fn set_track(&mut self, track: Option<SoundTrack>) -> Result<(), &'static str> {
        if self.track == track {
            return Ok(());
        }
        self.stop();
        self.track = track;
        let Some(track) = track else { return Ok(()) };
        let bytes: &[u8] = match track {
            SoundTrack::LightRain => include_bytes!("../../assets/audio/light-rain.wav"),
            SoundTrack::CoffeeShop => include_bytes!("../../assets/audio/coffee-shop.wav"),
            SoundTrack::Library => include_bytes!("../../assets/audio/library.wav"),
            SoundTrack::Fireplace => include_bytes!("../../assets/audio/fireplace.wav"),
        };
        let data = NSData::with_bytes(bytes);
        let sound = NSSound::initWithData(NSSound::alloc(), &data).ok_or("无法读取白噪音音源。")?;
        sound.setLoops(true);
        if !sound.play() {
            return Err("白噪音播放失败，请检查音频输出后重试。");
        }
        self.sound = Some(sound);
        Ok(())
    }

    pub(super) fn stop(&mut self) {
        if let Some(sound) = self.sound.take() {
            sound.stop();
        }
        self.track = None;
    }
}
