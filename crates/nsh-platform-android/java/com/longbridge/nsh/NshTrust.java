package com.longbridge.nsh;

import java.io.File;

/**
 * The keys, revocations and accepted builds that signed packages
 * ({@link NshView#loadPackage}) are verified against. The newest revocation
 * list and the highest build accepted per app persist in its state
 * directory, and packages' assets are extracted under it. Trusts no key
 * until {@link #addRoot}. Call {@link #close} when done.
 */
public final class NshTrust implements AutoCloseable {
    private long handle;

    /** @throws IllegalStateException if the state in {@code stateDir} cannot be read */
    public NshTrust(File stateDir) {
        handle = Native.trustCreate(stateDir.getAbsolutePath());
        if (handle == 0) throw new IllegalStateException("cannot open the trust store in " + stateDir);
    }

    /**
     * Trusts a root public key (base64, as `nsh keygen` writes it in
     * `<name>.pub`): packages whose release key it certified load. False for
     * an invalid key.
     */
    public boolean addRoot(String publicKey) {
        return Native.trustAddRoot(handle(), publicKey.trim());
    }

    /**
     * Applies a revocation list (the JSON `nsh revoke` writes) signed by a
     * trusted root and newer than the one the store has. False if it is
     * invalid, untrusted or not newer.
     */
    public boolean updateRevocations(String json) {
        return Native.trustUpdateRevocations(handle(), json);
    }

    long handle() {
        if (handle == 0) throw new IllegalStateException("NshTrust is closed");
        return handle;
    }

    @Override
    public synchronized void close() {
        if (handle != 0) {
            Native.trustDestroy(handle);
            handle = 0;
        }
    }
}
