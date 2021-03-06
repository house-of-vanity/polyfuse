# `polyfuse` examples

This directory contains a collection of filesystem examples using `polyfuse`.
Note that the primary purpose of these examples is to demonstrate the feature of `polyfuse` and they are not for production use.

To run the example:

```shell-session
$ cargo run -p polyfuse-examples --example <name> -- [<args>..]
```

### [`hello`](examples/hello.rs)
A read-only filesystem with a single file in the root directory.

### [`memfs`](examples/memfs.rs)
An in-memory filesystem that demonstrates a series of filesystem features, such as reading/writing regular files, creating, removing and renaming inodes, creating the hard/symbolic links, and acquiring/modifying the node attributes.
Some features such as file locking are omitted.

### [`passthrough`](examples/passthrough)
A filesystem that mirrors an existing directory structure to the root. This is a port of libfuse's `passthrough_hp.cc`, which manages the inode entries referenced by the kernel using the file descriptor with `O_PATH` flag.

### [`path_through`](examples/path_through.rs)
Another version of `passthrough` that holds the relative path from the root directory instead of the file descriptor.

### [`poll`](examples/poll.rs)
A filesystem that supports polling of events.
For simplicity, the root of filesystem uses a single file instead of a directory.

### [`heartbeat`](examples/heartbeat.rs)
A filesystem that demonstrates the notifications to the kernel.
In this example, the filesystem periodically updates the contents of the root file and then sends a notification message to the kernel to prompt for updating the page cache.
There are two kinds of notification: the one is to notify only that the cache data has been invalidated (`invalidate`), and the other is to send the range of updated data explicitly (`store`). These can be specified with the `--nofity-kind` command line option.

### [`heartbeat_entry`](examples/heartbeat_entry.rs)
A filesystem that notifies to the kernel that an entry has been deleted.
