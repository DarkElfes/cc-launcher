//! Windows bits: notifications, clipboard images, Recycle Bin, single instance.

use super::BackendEvent;
use std::path::PathBuf;

/// Shows a Windows notification. Clicking it sends NotificationClicked with the tab id.
pub fn notify(title: &str, body: &str, tab: &str, events: async_channel::Sender<BackendEvent>) {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::Toast;
        let tab = tab.to_string();
        let title: String = title.chars().take(120).collect();
        let body: String = body.chars().take(300).collect();
        std::thread::spawn(move || {
            let _ = Toast::new(Toast::POWERSHELL_APP_ID)
                .title(&title)
                .text1(&body)
                .on_activated(move |_| {
                    let _ = events.send_blocking(BackendEvent::NotificationClicked { tab: tab.clone() });
                    Ok(())
                })
                .show();
        });
    }
    #[cfg(not(windows))]
    {
        let _ = (title, body, tab, events);
    }
}

pub fn pasted_images_dir() -> PathBuf {
    super::store::data_dir().join("pasted-images")
}

/// If the clipboard holds an image, saves it as a PNG file and returns its path.
pub fn save_clipboard_image() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let img = cb.get_image().ok()?;
    let buf = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())?;
    let dir = pasted_images_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("paste-{}.png", chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S-%3fZ"));
    let file = dir.join(name);
    buf.save(&file).ok()?;
    Some(file.to_string_lossy().to_string())
}

/// Pasted screenshots are only needed while Claude reads them; drop ones older than 14 days.
pub fn prune_pasted_images() {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(14 * 86_400);
    let Ok(rd) = std::fs::read_dir(pasted_images_dir()) else { return };
    for e in rd.flatten() {
        if e.metadata().and_then(|m| m.modified()).map(|m| m < cutoff).unwrap_or(false) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub fn move_to_trash(path: &str) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

/// True when this is the only running copy. A second copy pokes the first one and should quit.
pub fn single_instance(events: async_channel::Sender<BackendEvent>) -> bool {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    let port_file = super::store::data_dir().join("instance.port");
    // Someone already running? Ask them to show their window.
    if let Ok(p) = std::fs::read_to_string(&port_file) {
        if let Ok(port) = p.trim().parse::<u16>() {
            if let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), std::time::Duration::from_millis(300)) {
                let _ = s.write_all(b"cc-launcher:activate");
                let mut ack = [0u8; 2];
                if s.read_exact(&mut ack).is_ok() && &ack == b"ok" {
                    return false;
                }
            }
        }
    }
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else { return true };
    if let Ok(addr) = listener.local_addr() {
        let _ = std::fs::create_dir_all(super::store::data_dir());
        let _ = std::fs::write(&port_file, addr.port().to_string());
    }
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 20];
            if s.read_exact(&mut buf).is_ok() && &buf == b"cc-launcher:activate" {
                let _ = s.write_all(b"ok");
                let _ = events.send_blocking(BackendEvent::Activate);
            }
        }
    });
    true
}

/// Opens Windows voice typing (Win+H) for the focused text field. Speech goes to Microsoft's
/// online recognizer and types into the field; its language follows the Windows input language.
pub fn start_voice_typing() {
    #[cfg(windows)]
    {
        const VK_LWIN: u8 = 0x5B;
        const VK_H: u8 = 0x48;
        const KEYEVENTF_KEYUP: u32 = 0x0002;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
        }
        // SAFETY: plain synthesized key presses (Win down, H down/up, Win up).
        unsafe {
            keybd_event(VK_LWIN, 0, 0, 0);
            keybd_event(VK_H, 0, 0, 0);
            keybd_event(VK_H, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_LWIN, 0, KEYEVENTF_KEYUP, 0);
        }
    }
}
