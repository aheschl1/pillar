export const toHex = (bytes) => {
    if (typeof bytes === 'string') return bytes;
    return bytes.reduce((str, byte) => str + byte.toString(16).padStart(2, '0'), '');
};

/** 32 bytes as 64 hex chars (an optional 0x is dropped), or null */
export const hexToBytes = (hex) => {
    const clean = hex.replace(/^0x/, '').trim().toLowerCase();
    if (!/^[0-9a-f]{64}$/.test(clean)) return null;
    const out = [];
    for (let i = 0; i < 64; i += 2) out.push(parseInt(clean.substr(i, 2), 16));
    return out;
};

export const shortHex = (hex) => `${hex.slice(0, 8)}…${hex.slice(-6)}`;

/** block timestamps are seconds since the epoch; genesis is 0 */
export const formatTimestamp = (seconds) =>
    seconds ? new Date(seconds * 1000).toLocaleString() : '—';

// pages to open a block or an account at
export const blockLink = (hash) => `/block?hash=${toHex(hash)}`;
export const accountLink = (address) => `/account?address=${toHex(address)}`;
