#![allow(warnings)]

use std::ffi::CString;
use std::ptr;

// Переносим генерацию биндингов строго сюда
mod dpdk {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

// Безопасная обертка над пулом памяти DPDK
pub struct Mempool {
    raw: *mut dpdk::rte_mempool,
}

// Безопасная обертка над сетевым портом
pub struct DpdkPort {
    id: u16,
}

// Безопасная обертка над пакетом буфера
pub struct PacketBuffer {
    raw: *mut dpdk::rte_mbuf,
}

impl DpdkPort {
    pub fn init_vdev(iface_name: &str, cores: &str, _ip_to_find: [u8; 4]) -> Result<(Self, Mempool), String> {
        let vdev_arg = format!("net_tap0,iface={}", iface_name);
        let args = vec!["dpdk-rust-app", "-l", cores, "--vdev", &vdev_arg];

        let c_args: Vec<CString> = args.iter().map(|s| CString::new(*s).unwrap()).collect();
        let mut arg_ptrs: Vec<*mut i8> = c_args.iter().map(|c| c.as_ptr() as *mut i8).collect();

        unsafe {
            if dpdk::rte_eal_init(arg_ptrs.len() as i32, arg_ptrs.as_mut_ptr()) < 0 {
                return Err("Ошибка инициализации EAL".to_string());
            }

            let pool_name = CString::new("MBUF_POOL").unwrap();
            let raw_pool = dpdk::rte_pktmbuf_pool_create(
                pool_name.as_ptr(),
                8191, 256, 0,
                dpdk::RTE_MBUF_DEFAULT_BUF_SIZE as u16,
                0
            );
            if raw_pool.is_null() {
                return Err("Ошибка создания Mempool".to_string());
            }
            let mempool = Mempool { raw: raw_pool };

            let port_id = 0u16;
            let mut port_conf: dpdk::rte_eth_conf = std::mem::zeroed();

            if dpdk::rte_eth_dev_configure(port_id, 1, 1, &port_conf) < 0 { return Err("Ошибка Config".to_string()); }
            if dpdk::rte_eth_rx_queue_setup(port_id, 0, 1024, 0, ptr::null(), raw_pool) < 0 { return Err("Ошибка RX".to_string()); }
            if dpdk::rte_eth_tx_queue_setup(port_id, 0, 1024, 0, ptr::null()) < 0 { return Err("Ошибка TX".to_string()); }
            if dpdk::rte_eth_dev_start(port_id) < 0 { return Err("Ошибка Start".to_string()); }

            Ok((DpdkPort { id: port_id }, mempool))
        }
    }

    pub fn rx_burst(&self, max_packets: usize) -> Vec<PacketBuffer> {
        let mut raw_mbufs: Vec<*mut dpdk::rte_mbuf> = vec![ptr::null_mut(); max_packets];
        unsafe {
            let nb_rx = dpdk::wrap_rte_eth_rx_burst(self.id, 0, raw_mbufs.as_mut_ptr(), max_packets as u16);
            raw_mbufs.into_iter()
                .take(nb_rx as usize)
                .filter(|ptr| !ptr.is_null())
                .map(|ptr| PacketBuffer { raw: ptr })
                .collect()
        }
    }

    pub fn tx_send(&self, packet: PacketBuffer) -> bool {
        unsafe {
            let mut tx_buffer = [packet.raw; 1];
            let nb_tx = dpdk::wrap_rte_eth_tx_burst(self.id, 0, tx_buffer.as_mut_ptr(), 1);
            if nb_tx > 0 {
                // Забываем про владение, так как памятью теперь управляет Си-драйвер
                std::mem::forget(packet);
                true
            } else {
                false
            }
        }
    }
}

impl PacketBuffer {
    pub fn as_slice(&self) -> &[u8] {
        unsafe {
            let data_ptr = ((*self.raw).buf_addr as *const u8).add((*self.raw).data_off as usize);
            let data_len = (*self.raw).data_len as usize;
            std::slice::from_raw_parts(data_ptr, data_len)
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe {
            let data_ptr = ((*self.raw).buf_addr as *mut u8).add((*self.raw).data_off as usize);
            let data_len = (*self.raw).data_len as usize;
            std::slice::from_raw_parts_mut(data_ptr, data_len)
        }
    }
}

impl Drop for PacketBuffer {
    fn drop(&mut self) {
        unsafe {
            dpdk::wrap_rte_pktmbuf_free_seg(self.raw);
        }
    }
}
