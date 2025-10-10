import React, { useState, useEffect, useCallback } from 'react';
import { useServer } from '../contexts/serverContext';
import BlockComponent from '../components/BlockComponent';
import { toHex } from '../api/utils';
import TransactionView from '../components/TransactionView';
import StateView from '../components/StateView';
import { useLocation, useNavigate } from 'react-router-dom';
import './Block.css';

const hexClean = (s) => s.replace(/^0x/, '').trim();

const hexToBytes = (hex) => {
    const clean = hexClean(hex);
    if (clean.length !== 64) return null;
    const out = [];
    for (let i = 0; i < 64; i += 2) {
        const byte = parseInt(clean.substr(i, 2), 16);
        if (Number.isNaN(byte)) return null;
        out.push(byte);
    }
    return out;
};

const Block = () => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const location = useLocation();
    const navigate = useNavigate();
    const params = new URLSearchParams(location.search);
    const initialHash = params.get('hash') || '';

    const [hashInput, setHashInput] = useState(initialHash);
    const [block, setBlock] = useState(null);
    const [txs, setTxs] = useState([]);
    const [accountStates, setAccountStates] = useState([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);

    const fetchBlock = useCallback(async (hash) => {
        if (!hash) return;
        setError(null);
        setBlock(null);
        setTxs([]);
        setAccountStates([]);

        const bytes = hexToBytes(hash);
        if (!bytes) {
            setError('Block hash must be 32-byte hex (64 hex chars)');
            return;
        }
        const hexHash = toHex(bytes);

        if (!isConnected) {
            setError('Not connected to server');
            return;
        }

        setLoading(true);
        try {
            const res = await fetch(`http://${ipAddress}:${httpPort}/block/${hexHash}`);
            if (!res.ok) throw new Error(`HTTP status ${res.status}`);
            const body = await res.json();
            if (!body.success) throw new Error(body.error || 'Failed to fetch block');
            const b = body.body;
            setBlock(b);

            const txPromises = (b.transaction_hashs || []).map(async (txHashBytes) => {
                const txHashHex = toHex(txHashBytes);
                try {
                    const tres = await fetch(`http://${ipAddress}:${httpPort}/transaction/${hexHash}/${txHashHex}`);
                    if (!tres.ok) throw new Error(`HTTP status ${tres.status}`);
                    const tbody = await tres.json();
                    if (!tbody.success) throw new Error(tbody.error || 'Failed to fetch transaction');
                    return tbody.body;
                } catch (e) {
                    console.error('Failed to fetch tx', txHashHex, e);
                    return { error: e.message || String(e), hash: txHashHex };
                }
            });

            const fetchedTxs = await Promise.all(txPromises);
            setTxs(fetchedTxs);

            try {
                const sres = await fetch(`http://${ipAddress}:${httpPort}/state/${hexHash}`);
                if (!sres.ok) throw new Error(`HTTP status ${sres.status}`);
                const sbody = await sres.json();
                if (!sbody.success) throw new Error(sbody.error || 'Failed to fetch state');
                setAccountStates(sbody.body.accounts || []);
            } catch (e) {
                console.error('Failed to fetch state', e);
            }
        } catch (e) {
            setError(e.message || String(e));
        } finally {
            setLoading(false);
        }
    }, [ipAddress, httpPort, isConnected]);

    // Automatically load when ?hash= is present
    useEffect(() => {
        if (initialHash) {
            fetchBlock(initialHash);
        }
    }, [initialHash, fetchBlock]);

    const handleSearch = async (e) => {
        e.preventDefault();
        // Update the URL without refreshing
        navigate(`/block?hash=${hashInput}`);
        await fetchBlock(hashInput);
    };

    return (
        <div className="block-page">
            <div className="header">
                <h2>Block Explorer</h2>
            </div>

            <form className="block-search" onSubmit={handleSearch}>
                <input
                    value={hashInput}
                    onChange={(e) => setHashInput(e.target.value)}
                    placeholder="Enter block hash (64 hex chars)"
                    className="hash-input"
                />
                <button type="submit" className="search-btn" disabled={loading}>
                    {loading ? 'Searching...' : 'Search'}
                </button>
            </form>

            {error && <div className="error-box">{error}</div>}

            {block && (
                <div className="block-grid">
                    <div className="card">
                        <h3>Block Details</h3>
                        <BlockComponent block={block} forceExpanded={true} />
                    </div>
                    <div className="card">
                        <h3>Transactions ({txs.length})</h3>
                        <TransactionView txs={txs} />
                    </div>
                    <div className="card">
                        <h3>Account States</h3>
                        <StateView accounts={accountStates} />
                    </div>
                </div>
            )}
        </div>
    );
};

export default Block;
