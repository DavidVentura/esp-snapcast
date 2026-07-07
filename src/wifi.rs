use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_sys::esp;
use esp_idf_sys::{esp_wifi_set_ps, wifi_ps_type_t_WIFI_PS_NONE};

use esp_idf_svc::nvs::{EspNvsPartition, NvsDefault};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_svc::wifi::{ClientConfiguration, Configuration};
use esp_idf_sys::EspError;

/// The nvs stores the RF calibration data, which allows for faster connection
pub(crate) fn configure(
    ssid: &str,
    pass: &str,
    nvs: EspNvsPartition<NvsDefault>,
    modem: &mut Modem,
) -> Result<[u8; 6], EspError> {
    // Configure Wifi
    let sysloop = EspSystemEventLoop::take()?;

    // logs association drops (e.g. reason 16 = group key handshake timeout during
    // the AP's hourly GTK rekey) with reason code and RSSI
    let sub = sysloop.subscribe::<esp_idf_svc::wifi::WifiEvent, _>(|event| match event {
        esp_idf_svc::wifi::WifiEvent::StaDisconnected(d) => {
            log::warn!("wifi disconnected: {d:?}")
        }
        esp_idf_svc::wifi::WifiEvent::StaConnected(c) => log::info!("wifi connected: {c:?}"),
        _ => (),
    })?;
    std::mem::forget(sub);

    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: heapless::String::try_from(ssid).unwrap(),
        password: heapless::String::try_from(pass).unwrap(),
        ..Default::default()
    }))?;

    wifi.start()?;
    // disable radio power saving; makes connectivity generally faster
    esp!(unsafe { esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE) })?;
    wifi.connect()?;

    // Wait until the network interface is up
    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    let mac = wifi.wifi().ap_netif().get_mac()?;
    log::info!("IP config: {:?}", ip_info);
    std::mem::forget(wifi);
    Ok(mac)
}
