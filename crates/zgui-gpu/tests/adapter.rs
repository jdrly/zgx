//! Windows: the shared device is on the adapter DXGI ranks first for low
//! power, the one wgpu would rank first, found without opening a device on
//! the others.
#![cfg(windows)]
use windows::Win32::Graphics::Dxgi::*;

#[test]
fn the_device_is_on_dxgis_first_adapter_for_low_power() {
    let info = zgui_gpu::GpuRenderer::new(8, 8).unwrap().adapter_info();
    if info.backend != wgpu::Backend::Dx12 {
        return;
    }
    let factory: IDXGIFactory6 =
        unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.unwrap();
    let first: IDXGIAdapter1 =
        unsafe { factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_MINIMUM_POWER) }
            .unwrap();
    let desc = unsafe { first.GetDesc1() }.unwrap();
    let name = String::from_utf16_lossy(&desc.Description);
    assert_eq!(name.trim_end_matches('\0'), info.name);
    assert_eq!((desc.VendorId, desc.DeviceId), (info.vendor, info.device));
}
