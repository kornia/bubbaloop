use axum::response::{IntoResponse, Json};
use serde::Serialize;

/// The current user's information
#[derive(Debug, Serialize)]
struct Whoami {
    arch: WhoamiArch,
    distro: String,
    desktop_env: WhoamiDesktopEnv,
    device_name: String,
    hostname: String,
    platform: WhoamiPlatform,
    realname: String,
    username: String,
}

/// Get the current user's information
pub async fn whoami() -> impl IntoResponse {
    log::debug!("🤖 Received request for whoami");
    Json(Whoami {
        realname: whoami::realname(),
        username: whoami::username(),
        hostname: match whoami::fallible::hostname() {
            Ok(hostname) => hostname,
            Err(_) => "unknown".to_string(),
        },
        platform: WhoamiPlatform::from_whoami_platform(whoami::platform()),
        arch: WhoamiArch::from_whoami_arch(whoami::arch()),
        distro: whoami::distro(),
        device_name: whoami::devicename(),
        desktop_env: WhoamiDesktopEnv::from_whoami_desktop_env(whoami::desktop_env()),
    })
}

/// The architecture of the current system
#[derive(Debug, Serialize)]
enum WhoamiArch {
    Arm64,
    Armv5,
    Armv6,
    Armv7,
    I386,
    I586,
    I686,
    X64,
    Mips,
    Mipsel,
    Mips64,
    Mips64el,
    Powerpc,
    Powerpc64,
    Powerpc64le,
    Riscv32,
    Riscv64,
    S390x,
    Sparc,
    Sparc64,
    Wasm32,
    Wasm64,
    Unknown,
}

impl WhoamiArch {
    fn from_whoami_arch(arch: whoami::Arch) -> Self {
        match arch {
            whoami::Arch::Arm64 => Self::Arm64,
            whoami::Arch::ArmV5 => Self::Armv5,
            whoami::Arch::ArmV6 => Self::Armv6,
            whoami::Arch::ArmV7 => Self::Armv7,
            whoami::Arch::I386 => Self::I386,
            whoami::Arch::I586 => Self::I586,
            whoami::Arch::I686 => Self::I686,
            whoami::Arch::X64 => Self::X64,
            whoami::Arch::Mips => Self::Mips,
            whoami::Arch::MipsEl => Self::Mipsel,
            whoami::Arch::Mips64 => Self::Mips64,
            whoami::Arch::Mips64El => Self::Mips64el,
            whoami::Arch::PowerPc => Self::Powerpc,
            whoami::Arch::PowerPc64 => Self::Powerpc64,
            whoami::Arch::PowerPc64Le => Self::Powerpc64le,
            whoami::Arch::Riscv32 => Self::Riscv32,
            whoami::Arch::Riscv64 => Self::Riscv64,
            whoami::Arch::S390x => Self::S390x,
            whoami::Arch::Sparc => Self::Sparc,
            whoami::Arch::Sparc64 => Self::Sparc64,
            whoami::Arch::Wasm32 => Self::Wasm32,
            whoami::Arch::Wasm64 => Self::Wasm64,
            whoami::Arch::Unknown(_) => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}

/// The platform of the current system
#[derive(Debug, Serialize)]
enum WhoamiPlatform {
    Linux,
    Bsd,
    Windows,
    Macos,
    Illumos,
    Ios,
    Android,
    Nintendo,
    Xbox,
    PlayStation,
    Fuchsia,
    Redox,
    Unknown,
}

impl WhoamiPlatform {
    fn from_whoami_platform(platform: whoami::Platform) -> Self {
        match platform {
            whoami::Platform::Linux => Self::Linux,
            whoami::Platform::Bsd => Self::Bsd,
            whoami::Platform::Windows => Self::Windows,
            whoami::Platform::MacOS => Self::Macos,
            whoami::Platform::Illumos => Self::Illumos,
            whoami::Platform::Ios => Self::Ios,
            whoami::Platform::Android => Self::Android,
            whoami::Platform::Nintendo => Self::Nintendo,
            whoami::Platform::Xbox => Self::Xbox,
            whoami::Platform::PlayStation => Self::PlayStation,
            whoami::Platform::Fuchsia => Self::Fuchsia,
            whoami::Platform::Redox => Self::Redox,
            whoami::Platform::Unknown(_) => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}

/// The desktop environment of the current system
#[derive(Debug, Serialize)]
enum WhoamiDesktopEnv {
    Gnome,
    Windows,
    Lxde,
    Openbox,
    Mate,
    Xfce,
    Kde,
    Cinnamon,
    I3,
    Aqua,
    Ios,
    Android,
    WebBrowser,
    Console,
    Ubuntu,
    Ermine,
    Orbital,
    Unknown,
}

impl WhoamiDesktopEnv {
    fn from_whoami_desktop_env(desktop_env: whoami::DesktopEnv) -> Self {
        match desktop_env {
            whoami::DesktopEnv::Gnome => Self::Gnome,
            whoami::DesktopEnv::Windows => Self::Windows,
            whoami::DesktopEnv::Lxde => Self::Lxde,
            whoami::DesktopEnv::Openbox => Self::Openbox,
            whoami::DesktopEnv::Mate => Self::Mate,
            whoami::DesktopEnv::Xfce => Self::Xfce,
            whoami::DesktopEnv::Kde => Self::Kde,
            whoami::DesktopEnv::Cinnamon => Self::Cinnamon,
            whoami::DesktopEnv::I3 => Self::I3,
            whoami::DesktopEnv::Aqua => Self::Aqua,
            whoami::DesktopEnv::Ios => Self::Ios,
            whoami::DesktopEnv::Android => Self::Android,
            whoami::DesktopEnv::WebBrowser => Self::WebBrowser,
            whoami::DesktopEnv::Console => Self::Console,
            whoami::DesktopEnv::Ubuntu => Self::Ubuntu,
            whoami::DesktopEnv::Ermine => Self::Ermine,
            whoami::DesktopEnv::Orbital => Self::Orbital,
            whoami::DesktopEnv::Unknown(_) => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}
