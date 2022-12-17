use crate::{device::DeviceWrapper, Error, NetworkDevice, Result};
use alloc::{collections::BTreeMap, sync::Arc};
use irq_safety::MutexIrqSafe;
use mutex_sleep::MutexSleep;
use smoltcp::{iface, phy::DeviceCapabilities, wire};

pub use smoltcp::iface::SocketSet;
pub use wire::{IpAddress, IpCidr};

/// A network interface.
///
/// This is a wrapper around a network device which provides higher level
/// abstractions such as polling sockets.
#[derive(Clone)]
pub struct NetworkInterface {
    // FIXME: Can this be a regular mutex?
    inner: Arc<MutexSleep<iface::Interface<'static>>>,
    device: &'static MutexIrqSafe<dyn crate::NetworkDevice>,
}

impl NetworkInterface {
    pub(crate) fn new<T>(device: &'static MutexIrqSafe<T>, ip: IpCidr, gateway: IpAddress) -> Self
    where
        T: NetworkDevice,
    {
        let hardware_addr = wire::EthernetAddress(device.lock().mac_address()).into();

        let mut routes = iface::Routes::new(BTreeMap::new());

        match gateway {
            IpAddress::Ipv4(addr) => routes.add_default_ipv4_route(addr),
            IpAddress::Ipv6(addr) => routes.add_default_ipv6_route(addr),
        }
        .expect("btree map route storage exhausted");

        let mut wrapper = DeviceWrapper {
            inner: &mut *device.lock(),
        };
        let inner = Arc::new(MutexSleep::new(
            iface::InterfaceBuilder::new()
                .random_seed(random::next_u64())
                .hardware_addr(hardware_addr)
                .ip_addrs([ip])
                .routes(routes)
                .neighbor_cache(iface::NeighborCache::new(BTreeMap::new()))
                .finalize(&mut wrapper),
        ));

        Self { inner, device }
    }

    /// Transmit and receive any packets queued in the given `sockets`.
    ///
    /// Returns a boolean value indicating whether any packets were processed or
    /// emitted, and thus, whether the readiness of any socket might have
    /// changed.
    pub fn poll(&self, sockets: &mut SocketSet) -> Result<bool> {
        let mut wrapper = DeviceWrapper {
            inner: &mut *self.device.lock(),
        };
        // FIXME: Timestamp
        self.inner
            .lock()
            .map_err(|_| Error::Unknown)?
            .poll(smoltcp::time::Instant::ZERO, &mut wrapper, sockets)
            .map_err(|e| e.into())
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        self.device.lock().capabilities()
    }
}