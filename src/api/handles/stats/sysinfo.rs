use axum::response::{IntoResponse, Json};
use serde::Serialize;
use sysinfo::{Disks, System};

#[derive(Debug, Serialize)]
struct Sysinfo {
    // memory
    total_memory: u64,
    free_memory: u64,
    used_memory: u64,
    available_memory: u64,
    total_swap: u64,
    // system
    name: String,
    kernel_version: String,
    os_version: String,
    host_name: String,
    cpus: Vec<Cpu>,
    disks: Vec<Disk>,
    global_cpu_usage: f32,
}

#[derive(Debug, Serialize)]
struct Cpu {
    name: String,
    brand: String,
    frequency: u64,
    usage: f32,
}

#[derive(Debug, Serialize)]
struct Disk {
    name: String,
    file_system: String,
    mount_point: String,
    total_space: u64,
    available_space: u64,
}

pub async fn get_sysinfo() -> impl IntoResponse {
    log::debug!("🤖 Received request for sysinfo");

    // TODO: implement this into a background task
    let mut sys = System::new_all();
    sys.refresh_all();
    sys.refresh_cpu_usage();

    let mut disks = Vec::new();
    for sys_disk in &Disks::new_with_refreshed_list() {
        disks.push(Disk {
            name: sys_disk.name().to_string_lossy().to_string(),
            file_system: sys_disk.file_system().to_string_lossy().to_string(),
            mount_point: sys_disk.mount_point().to_string_lossy().to_string(),
            total_space: sys_disk.total_space(),
            available_space: sys_disk.available_space(),
        });
    }

    let mut cpus = Vec::new();
    for cpu in sys.cpus() {
        cpus.push(Cpu {
            name: cpu.name().to_string(),
            brand: cpu.brand().to_string(),
            frequency: cpu.frequency(),
            usage: cpu.cpu_usage(),
        });
    }

    Json(Sysinfo {
        total_memory: sys.total_memory(),
        free_memory: sys.free_memory(),
        used_memory: sys.used_memory(),
        available_memory: sys.available_memory(),
        total_swap: sys.total_swap(),
        name: System::name().unwrap_or_default(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        os_version: System::os_version().unwrap_or_default(),
        host_name: System::host_name().unwrap_or_default(),
        cpus,
        disks,
        global_cpu_usage: sys.global_cpu_usage(),
    })
}
