use std::env;
use std::path::PathBuf;

fn main() {
    // 1. Опрашиваем pkg-config
    let library = pkg_config::Config::new()
        .atleast_version("23.11")
        .probe("libdpdk")
        .expect("Ошибка: В системе не найден libdpdk-dev");

    // 2. Компилируем нашу Си-обертку с флагами архитектуры CPU
    let mut cc_build = cc::Build::new();
    cc_build.file("src/dpdk_wrapper.c");
    for path in &library.include_paths {
        cc_build.include(path);
    }
    cc_build.include("/usr/include/x86_64-linux-gnu/dpdk");
    cc_build.include("/usr/include/dpdk");

    // Включаем векторные инструкции (SSSE3/AVX), подстраиваясь под ваш текущий процессор
    cc_build.flag("-march=native");

    cc_build.compile("dpdk_wrapper");

    // 3. Запускаем bindgen и тоже прокидываем флаг архитектуры
    let bindings = bindgen::Builder::default()
        .header("/usr/include/dpdk/rte_eal.h")
        .header("/usr/include/dpdk/rte_ethdev.h")
        .header("src/dpdk_wrapper.c")
        .clang_arg("-I/usr/include/dpdk")
        .clang_arg("-I/usr/include/x86_64-linux-gnu/dpdk")
        // Критически важно: передаем bindgen/clang инструкцию использовать native CPU расширения
        .clang_arg("-march=native")
        .allowlist_type("rte_mbuf")
        .allowlist_type("rte_eth_conf")
        .allowlist_function("rte_eal_init")
        .allowlist_function("rte_pktmbuf_pool_create")
        .allowlist_function("rte_eth_dev_configure")
        .allowlist_function("rte_eth_rx_queue_setup")
        .allowlist_function("rte_eth_tx_queue_setup")
        .allowlist_function("rte_eth_dev_start")
        .allowlist_function("wrap_rte_eth_rx_burst")
        .allowlist_function("wrap_rte_pktmbuf_free_seg")
        .allowlist_function("wrap_rte_eth_tx_burst")
        .allowlist_var("RTE_MBUF_DEFAULT_BUF_SIZE")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Не удалось сгенерировать привязки к DPDK");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_path.join("bindings.rs")).expect("Не удалось записать bindings.rs");
}
