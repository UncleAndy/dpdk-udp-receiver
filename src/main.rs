#![allow(warnings)]
use std::ffi::CString;
use std::ptr;
use anyhow::{Result, anyhow};

// Импортируем автоматически сгенерированные биндинги из build.rs
mod dpdk {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

fn main() -> Result<()> {
    // 1. Аргументы запуска EAL (Environment Abstraction Layer)
    // Создаем виртуальное устройство net_tap0, которое создаст интерфейс dpdk-tap0 в Ubuntu
    let args = vec!["dpdk-rust-app", "-l", "0-1", "--vdev", "net_tap0,iface=dpdk-tap0"];
    let c_args: Vec<CString> = args.iter().map(|s| CString::new(*s).unwrap()).collect();
    let mut arg_ptrs: Vec<*mut i8> = c_args.iter().map(|c| c.as_ptr() as *mut i8).collect();

    let port_id: u16 = 0;

    unsafe {
        // 2. Инициализация EAL ядра DPDK
        if dpdk::rte_eal_init(arg_ptrs.len() as i32, arg_ptrs.as_mut_ptr()) < 0 {
            return Err(anyhow!("Ошибка инициализации EAL"));
        }

        // 3. Создание пула памяти для сетевых буферов (Mempool)
        let pool_name = CString::new("MBUF_POOL").unwrap();
        let mbuf_pool = dpdk::rte_pktmbuf_pool_create(
            pool_name.as_ptr(),
            8191, // Количество mbuf в пуле
            256,  // Размер кэша для каждого ядра процессора
            0,
            dpdk::RTE_MBUF_DEFAULT_BUF_SIZE as u16, // Дефолтный размер буфера пакета (~2КБ)
            0     // Идентификатор NUMA-узла (0 - авто)
        );
        if mbuf_pool.is_null() {
            return Err(anyhow!("Ошибка создания пула памяти Mempool"));
        }

        // 4. Конфигурация и запуск виртуального порта
        let mut port_conf: dpdk::rte_eth_conf = std::mem::zeroed();

        // Настраиваем порт на 1 приемную (RX) и 1 передающую (TX) очереди
        if dpdk::rte_eth_dev_configure(port_id, 1, 1, &port_conf) < 0 {
            return Err(anyhow!("Ошибка конфигурации виртуального порта"));
        }

        // Настраиваем 0-ю RX очередь
        if dpdk::rte_eth_rx_queue_setup(port_id, 0, 1024, 0, ptr::null(), mbuf_pool) < 0 {
            return Err(anyhow!("Ошибка настройки очереди приема (RX)"));
        }

        // Настраиваем 0-ю TX очередь
        if dpdk::rte_eth_tx_queue_setup(port_id, 0, 1024, 0, ptr::null()) < 0 {
            return Err(anyhow!("Ошибка настройки очереди передачи (TX)"));
        }

        // Физически включаем порт в работу
        if dpdk::rte_eth_dev_start(port_id) < 0 {
            return Err(anyhow!("Не удалось запустить виртуальный порт"));
        }
    }

    println!("Успех! Локальный DPDK на Rust запущен.");
    println!("Слушаю интерфейс dpdk-tap0. Автоответчик ARP для 192.168.100.99 активен!");
    println!("Нажмите Ctrl+C для выхода.");

    // Массив, куда DPDK будет складывать указатели на пойманные mbuf-пакеты (до 32 за раз)
    let mut mbufs: [*mut dpdk::rte_mbuf; 32] = [ptr::null_mut(); 32];

    // Наш вымышленный MAC-адрес приложения: AA:BB:CC:DD:EE:FF
    let my_mac: [u8; 6] = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    // Наш вымышленный IP-адрес приложения: 192.168.100.99
    let my_ip: [u8; 4] = [192, 168, 100, 99];

    // 5. Бесконечный цикл опроса (Poll Mode Driver)
    loop {
        unsafe {
            // Опрашиваем 0-ю очередь приема 0-го порта через нашу Си-обертку
            let nb_rx = dpdk::wrap_rte_eth_rx_burst(port_id, 0, mbufs.as_mut_ptr(), 32);

            if nb_rx > 0 {
                for i in 0..nb_rx as usize {
                    let mbuf = mbufs[i];
                    if mbuf.is_null() { continue; }

                    // Вычисляем точный адрес начала данных пакета в оперативной памяти и его длину
                    let data_ptr = ((*mbuf).buf_addr as *mut u8).add((*mbuf).data_off as usize);
                    let data_len = (*mbuf).data_len as usize;

                    // Создаем изменяемый Rust-срез (mutable slice) над сырыми байтами пакета
                    let packet_slice = std::slice::from_raw_parts_mut(data_ptr, data_len);

                    if data_len >= 42 {
                        let eth_type = u16::from_be_bytes([packet_slice[12], packet_slice[13]]);

                        // Обработка UDP (без изменений)
                        if eth_type == 0x0800 && packet_slice[23] == 17 {
                            let src_port = u16::from_be_bytes([packet_slice[34], packet_slice[35]]);
                            let dst_port = u16::from_be_bytes([packet_slice[36], packet_slice[37]]);
                            println!(" Пойман UDP Пакет! Порты: {} -> {}.", src_port, dst_port);

                            let payload = &packet_slice[42..];

                            // Пытаемся преобразовать байты в читаемый текст UTF-8
                            if let Ok(text) = std::str::from_utf8(payload) {
                                // .trim() убирает переносы строк (\n), которые добавляет nc
                                println!("   Текст сообщения: \"{}\"", text.trim());
                            } else {
                                // Если прилетели бинарные данные, выводим их в шестнадцатеричном виде
                                print!("   Данные (HEX): ");
                                for byte in payload {
                                    print!("{:02X} ", byte);
                                }
                                println!();
                            }

                            dpdk::wrap_rte_pktmbuf_free_seg(mbuf);
                            continue;
                        }

                        // Улучшенная обработка ARP
                        if eth_type == 0x0806 {
                            let arp_op = u16::from_be_bytes([packet_slice[20], packet_slice[21]]);
                            let target_ip = &packet_slice[38..42];

                            println!("--> Поймали ARP-пакет! Операция: {} (1=запрос), Ищет IP: {}.{}.{}.{}",
                                     arp_op, target_ip[0], target_ip[1], target_ip[2], target_ip[3]
                            );

                            if arp_op == 1 && target_ip == my_ip {
                                println!("    [МАТЧ!] IP совпал с нашим. Переписываем кадр...");

                                let sender_mac: [u8; 6] = packet_slice[22..28].try_into().unwrap();
                                let sender_ip: [u8; 4] = packet_slice[28..32].try_into().unwrap();

                                // Переписываем заголовки
                                packet_slice[0..6].copy_from_slice(&sender_mac);
                                packet_slice[6..12].copy_from_slice(&my_mac);
                                packet_slice[20..22].copy_from_slice(&2u16.to_be_bytes()); // Reply

                                packet_slice[22..28].copy_from_slice(&my_mac);
                                packet_slice[28..32].copy_from_slice(&my_ip);
                                packet_slice[32..38].copy_from_slice(&sender_mac);
                                packet_slice[38..42].copy_from_slice(&sender_ip);

                                // Отправка
                                let mut tx_buffer = [mbuf; 1];
                                let nb_tx = dpdk::wrap_rte_eth_tx_burst(port_id, 0, tx_buffer.as_mut_ptr(), 1);

                                if nb_tx > 0 {
                                    println!("    [УСПЕХ] ARP-ответ ушел в TX-очередь.");
                                } else {
                                    println!("    [ОШИБКА] Драйвер не смог отправить пакет.");
                                    dpdk::wrap_rte_pktmbuf_free_seg(mbuf);
                                }
                                continue;
                            }
                        }
                    }

                    // Освобождаем память для всех остальных неинтересных пакетов (IPv6, мультикаст, чужие ARP)
                    dpdk::wrap_rte_pktmbuf_free_seg(mbuf);
                }
            }
        }
    }
}
