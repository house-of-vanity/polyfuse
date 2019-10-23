#![cfg(feature = "with-tokio")]
#![cfg_attr(feature = "docs", doc(cfg(feature = "with-tokio")))]

use crate::{
    conn::{Connection, MountOptions},
    op::Operations,
};
use futures_io::{AsyncRead, AsyncWrite};
use futures_util::{future::Future, ready, select, stream::StreamExt};
use libc::c_int;
use mio::{unix::UnixReady, Ready};
use std::{
    cell::UnsafeCell,
    io::{self, IoSlice, IoSliceMut, Read, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};
use tokio_net::{
    signal::unix::{signal, SignalKind},
    util::PollEvented,
};
use tokio_sync::semaphore::{Permit, Semaphore};

/// Run a FUSE filesystem mounted on the specified path.
pub async fn mount<T>(
    ops: T,
    mointpoint: impl AsRef<Path>,
    mountopts: MountOptions,
) -> io::Result<()>
where
    T: for<'a> Operations<&'a [u8]>,
{
    let channel = Channel::open(mointpoint, mountopts)?;
    let sig = default_shutdown_signal()?;

    crate::main_loop(ops, channel, sig).await?;
    Ok(())
}

/// Create a signal future that captures some kind of signals.
pub fn default_shutdown_signal() -> io::Result<impl Future<Output = c_int> + Unpin> {
    let mut sighup = signal(SignalKind::hangup())?.into_future();
    let mut sigint = signal(SignalKind::interrupt())?.into_future();
    let mut sigterm = signal(SignalKind::terminate())?.into_future();
    let mut sigpipe = signal(SignalKind::pipe())?.into_future();

    Ok(Box::pin(async move {
        loop {
            select! {
                _ = sighup => {
                    log::debug!("Got SIGHUP");
                    return libc::SIGHUP;
                },
                _ = sigint => {
                    log::debug!("Got SIGINT");
                    return libc::SIGINT;
                },
                _ = sigterm => {
                    log::debug!("Got SIGTERM");
                    return libc::SIGTERM;
                },
                _ = sigpipe => {
                    log::debug!("Got SIGPIPE (and ignored)");
                    continue
                }
            }
        }
    }))
}

/// Asynchronous I/O object that communicates with the FUSE kernel driver.
#[derive(Debug)]
pub struct Channel {
    inner: Arc<Inner>,
    permit: Permit,
}

#[derive(Debug)]
struct Inner {
    conn: UnsafeCell<PollEvented<Connection>>,
    mountpoint: PathBuf,
    semaphore: Semaphore,
}

impl Clone for Channel {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            permit: Permit::new(),
        }
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        self.release_lock();
    }
}

unsafe impl Send for Channel {}

impl Channel {
    /// Open a new communication channel mounted to the specified path.
    pub fn open(mountpoint: impl AsRef<Path>, mountopts: MountOptions) -> io::Result<Self> {
        let mountpoint = mountpoint.as_ref();

        let conn = Connection::open(mountpoint, mountopts)?;

        Ok(Self {
            inner: Arc::new(Inner {
                conn: UnsafeCell::new(PollEvented::new(conn)),
                mountpoint: mountpoint.into(),
                semaphore: Semaphore::new(1),
            }),
            permit: Permit::new(),
        })
    }

    /// Return the mountpoint path.
    pub fn mountpoint(&self) -> &Path {
        &self.inner.mountpoint
    }

    fn poll_lock_with<F, R>(&mut self, cx: &mut task::Context, f: F) -> Poll<R>
    where
        F: FnOnce(&mut PollEvented<Connection>, &mut task::Context) -> Poll<R>,
    {
        ready!(self.poll_acquire_lock(cx));

        let conn = unsafe { &mut (*self.inner.conn.get()) };
        let ret = ready!(f(conn, cx));

        self.release_lock();

        Poll::Ready(ret)
    }

    fn poll_acquire_lock(&mut self, cx: &mut task::Context) -> Poll<()> {
        if self.permit.is_acquired() {
            return Poll::Ready(());
        }

        ready!(self.permit.poll_acquire(cx, &self.inner.semaphore))
            .unwrap_or_else(|e| unreachable!("{}", e));

        Poll::Ready(())
    }

    fn release_lock(&mut self) {
        if self.permit.is_acquired() {
            self.permit.release(&self.inner.semaphore);
        }
    }

    fn poll_read_with<F, R>(&mut self, cx: &mut task::Context<'_>, f: F) -> Poll<io::Result<R>>
    where
        F: FnOnce(&mut Connection) -> io::Result<R>,
    {
        self.poll_lock_with(cx, |conn, cx| {
            let mut ready = Ready::readable();
            ready.insert(UnixReady::error());
            ready!(conn.poll_read_ready(cx, ready))?;

            match f(conn.get_mut()) {
                Ok(ret) => Poll::Ready(Ok(ret)),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    conn.clear_read_ready(cx, ready)?;
                    Poll::Pending
                }
                Err(e) => Poll::Ready(Err(e)),
            }
        })
    }

    fn poll_write_with<F, R>(&mut self, cx: &mut task::Context<'_>, f: F) -> Poll<io::Result<R>>
    where
        F: FnOnce(&mut Connection) -> io::Result<R>,
    {
        self.poll_lock_with(cx, |conn, cx| {
            ready!(conn.poll_write_ready(cx))?;

            match f(conn.get_mut()) {
                Ok(ret) => Poll::Ready(Ok(ret)),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    conn.clear_write_ready(cx)?;
                    Poll::Pending
                }
                Err(e) => Poll::Ready(Err(e)),
            }
        })
    }
}

impl AsyncRead for Channel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        dst: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_with(cx, |fd| fd.read(dst))
    }

    fn poll_read_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        dst: &mut [IoSliceMut],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_with(cx, |fd| fd.read_vectored(dst))
    }
}

impl AsyncWrite for Channel {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        src: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_with(cx, |fd| fd.write(src))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        src: &[IoSlice],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_with(cx, |fd| fd.write_vectored(src))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        self.poll_write_with(cx, |fd| fd.flush())
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
