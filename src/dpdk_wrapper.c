#include <rte_config.h>
#include <rte_eal.h>
#include <rte_ethdev.h>
#include <rte_mbuf.h>

// Создаем нормальную Си-функцию, которая оборачивает инлайновую rx_burst
uint16_t wrap_rte_eth_rx_burst(uint16_t port_id, uint16_t queue_id, struct rte_mbuf **rx_pkts, uint16_t nb_pkts) {
    return rte_eth_rx_burst(port_id, queue_id, rx_pkts, nb_pkts);
}

// Создаем нормальную Си-функцию, которая оборачивает инлайновую free_seg
void wrap_rte_pktmbuf_free_seg(struct rte_mbuf *m) {
    rte_pktmbuf_free_seg(m);
}

uint16_t wrap_rte_eth_tx_burst(uint16_t port_id, uint16_t queue_id, struct rte_mbuf **tx_pkts, uint16_t nb_pkts) {
    return rte_eth_tx_burst(port_id, queue_id, tx_pkts, nb_pkts);
}
