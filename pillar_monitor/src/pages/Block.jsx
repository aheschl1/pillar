import React, { useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useServer } from '../contexts/serverContext';
import BlockComponent from '../components/BlockComponent';
import { toHex, hexToBytes } from '../api/utils';
import TransactionView from '../components/TransactionView';
import StateView from '../components/StateView';
import './Block.css';

// The block in the URL (?hash=...), or the chain's newest block when there is none.
const Block = () => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const [searchParams, setSearchParams] = useSearchParams();
    const hash = (searchParams.get('hash') || '').toLowerCase();
    const [hashInput, setHashInput] = useState(hash);
    const [block, setBlock] = useState(null);
    const [txs, setTxs] = useState([]);
    const [accountStates, setAccountStates] = useState([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);

    const api = useCallback(async (path) => {
        const res = await fetch(`http://${ipAddress}:${httpPort}${path}`);
        if (!res.ok) throw new Error(`HTTP status ${res.status}`);
        const body = await res.json();
        if (!body.success) throw new Error(body.error || `Request to ${path} failed`);
        return body.body;
    }, [ipAddress, httpPort]);

    const showBlock = (hex, replace = false) => {
        setSearchParams((params) => { params.set('hash', hex); return params; }, { replace });
    };

    useEffect(() => {
        setHashInput(hash);
        if (!isConnected) return;
        let cancelled = false;
        (async () => {
            setError(null);
            setLoading(true);
            setBlock(null);
            setTxs([]);
            setAccountStates([]);
            try {
                if (!hash) {
                    const [tip] = await api('/blocks?limit=1');
                    if (!cancelled && tip) showBlock(toHex(tip), true);
                    return;
                }
                const b = await api(`/block/${hash}`);
                if (cancelled) return;
                setBlock(b);
                const fetchedTxs = await Promise.all((b.transaction_hashs || []).map(async (txHashBytes) => {
                    const txHashHex = toHex(txHashBytes);
                    try {
                        return await api(`/transaction/${hash}/${txHashHex}`);
                    } catch (e) {
                        return { error: e.message || String(e), hash: txHashHex };
                    }
                }));
                if (cancelled) return;
                setTxs(fetchedTxs);
                try {
                    const state = await api(`/state/${hash}`);
                    if (!cancelled) setAccountStates(state.accounts || []);
                } catch (e) {
                    console.error('Failed to fetch state', e); // the block is still worth showing
                }
            } catch (e) {
                if (!cancelled) setError(e.message || String(e));
            } finally {
                if (!cancelled) setLoading(false);
            }
        })();
        return () => { cancelled = true; };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [hash, isConnected, api]);

    const handleSearch = (e) => {
        e.preventDefault();
        const bytes = hexToBytes(hashInput);
        if (!bytes) {
            setError('A block hash is 64 hex characters');
            return;
        }
        showBlock(toHex(bytes));
    };

    return (
        <div className="block-page">
            <div className="block-page-header">
                <h2>Block</h2>
                <form className="block-search" onSubmit={handleSearch}>
                    <input value={hashInput} onChange={(e) => setHashInput(e.target.value)} placeholder="Block hash (64 hex characters)" spellCheck={false} />
                    <button type="submit">Open</button>
                    <button type="button" className="secondary" onClick={() => setSearchParams((p) => { p.delete('hash'); return p; })}>Latest</button>
                </form>
            </div>
            {error && <div className="error-box">{error}</div>}
            {loading && <div className="small">Loading…</div>}

            {block && (
                <div className="block-grid">
                    <div className="block-left">
                        <BlockComponent block={block} forceExpanded={true} />
                        <StateView accounts={accountStates} />
                    </div>
                    <div className="block-right">
                        <h3>Transactions ({txs.length})</h3>
                        <TransactionView txs={txs} />
                    </div>
                </div>
            )}
        </div>
    );
};

export default Block;
