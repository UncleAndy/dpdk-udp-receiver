use anyhow::{Result, anyhow};

mod dpdk_safe;
use dpdk_safe::DpdkPort;

fn main() -> Result<()> {
    let my_mac: [u8; 6] = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let my_ip: [u8; 4] = [192, 168, 100, 99];

    // Инициализируем порт через безопасную абстракцию
    let (port, _mempool) = DpdkPort::init_vdev("dpdk-tap0", "0-1", my_ip)
        .map_err(|e| anyhow!(e))?;

    println!("Успех! Локальный DPDK на 100% безопасном Rust запущен.");
    println!("Слушаю интерфейс dpdk-tap0... Нажмите Ctrl+C для выхода.");

    loop {
        let packets = port.rx_burst(32);

        for mut packet in packets {
            let data_len = packet.as_slice().len();

            if data_len >= 42 {
                let packet_slice = packet.as_slice();

                // Ethernet заголовок: тип протокола на смещении 12 и 13
                let eth_type = u16::from_be_bytes([packet_slice[12], packet_slice[13]]);

                // 1. ОБРАБОТКА UDP (EtherType = 0x0800, Номер протокола IP на смещении 23 = 17)
                if eth_type == 0x0800 && packet_slice[23] == 17 {
                    // UDP порты начинаются на смещении 34 и 36
                    let src_port = u16::from_be_bytes([packet_slice[34], packet_slice[35]]);
                    let dst_port = u16::from_be_bytes([packet_slice[36], packet_slice[37]]);
                    println!(" Пойман UDP Пакет! Порты: {} -> {}. Размер: {} байт", src_port, dst_port, data_len);

                    if data_len > 42 {
                        let payload = &packet_slice[42..];
                        if let Ok(text) = std::str::from_utf8(payload) {
                            println!("   Текст: \"{}\"", text.trim());
                        }
                    }
                    continue;
                }

                // 2. ОБРАБОТКА ARP (EtherType = 0x0806)
                if eth_type == 0x0806 {
                    // Код операции ARP на смещении 20 и 21
                    let arp_op = u16::from_be_bytes([packet_slice[20], packet_slice[21]]);
                    let target_ip = &packet_slice[38..42];

                    if arp_op == 1 && target_ip == my_ip {
                        println!(" Получен ARP-запрос к 192.168.100.99. Отвечаем...");

                        let sender_mac: [u8; 6] = packet_slice[22..28].try_into().unwrap();
                        let sender_ip: [u8; 4] = packet_slice[28..32].try_into().unwrap();

                        let mut_slice = packet.as_mut_slice();
                        mut_slice[0..6].copy_from_slice(&sender_mac);
                        mut_slice[6..12].copy_from_slice(&my_mac);
                        mut_slice[20..22].copy_from_slice(&2u16.to_be_bytes());
                        mut_slice[22..28].copy_from_slice(&my_mac);
                        mut_slice[28..32].copy_from_slice(&my_ip);
                        mut_slice[32..38].copy_from_slice(&sender_mac);
                        mut_slice[38..42].copy_from_slice(&sender_ip);

                        port.tx_send(packet);
                        continue;
                    }
                }
            }
        }
    }
}
