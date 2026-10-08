import React, { useState, useEffect } from 'react';
import { useSearchParams, useNavigate } from 'react-router-dom';
import { useServer } from '../contexts/serverContext';
import { toHex, hexToBytes, blockLink, accountLink } from '../api/utils';
import Hash from '../components/Hash';
import './Account.css';

// Any account's balance and nonce at the chain's tip, and its transactions: ?address=...
const Account = () => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const [searchParams, setSearchParams] = useSearchParams();
    const navigate = useNavigate();
    const address = (searchParams.get('address') || '').toLowerCase();
    const [input, setInput] = useState(address);
    const [account, setAccount] = useState(null);
    const [history, setHistory] = useState(null);
    const [error, setError] = useState(null);
    const [loading, setLoading] = useState(false);

    useEffect(() => {
        setInput(address);
        setAccount(null);
        setHistory(null);
        setError(null);
        if (!address || !isConnected) return;
        let cancelled = false;
        setLoading(true);
        const get = (path) => fetch(`http://${ipAddress}:${httpPort}${path}`).then((res) => res.json());
        Promise.all([get(`/account/${address}`), get(`/account/${address}/transactions?limit=100`)])
            .then(([body, transactions]) => {
                if (cancelled) return;
                if (body.success) setAccount(body.body);
                else setError(body.error || 'Lookup failed');
                // an older node has no history endpoint; the balance is still worth showing
                if (transactions.success) setHistory(transactions.body);
            })
            .catch((e) => { if (!cancelled) setError(e.message); })
            .finally(() => { if (!cancelled) setLoading(false); });
        return () => { cancelled = true; };
    }, [address, isConnected, ipAddress, httpPort]);

    const handleSubmit = (e) => {
        e.preventDefault();
        const bytes = hexToBytes(input);
        if (!bytes) {
            setError('An address is 64 hex characters');
            return;
        }
        setSearchParams((params) => { params.set('address', toHex(bytes)); return params; });
    };

    return (
        <div className="account-page">
            <h2>Accounts</h2>
            <form className="account-search" onSubmit={handleSubmit}>
                <input value={input} onChange={(e) => setInput(e.target.value)} placeholder="Address (64 hex characters)" spellCheck={false} />
                <button type="submit">Look up</button>
            </form>
            {error && <div className="error-box">{error}</div>}
            {loading && <div className="small">Loading…</div>}

            {account && (
                <div className="account-card">
                    <div className="account-balance">{account.balance} <span>coins</span></div>
                    <div className="account-row"><span className="field-label">Address</span><Hash value={account.address} /></div>
                    <div className="account-row"><span className="field-label">Nonce</span><span>{account.nonce}</span></div>
                    {account.balance === 0 && account.nonce === 0 && (
                        <p className="small">The chain has no record of this address yet, so it holds nothing.</p>
                    )}
                    <div className="account-actions">
                        <button type="button" className="secondary" onClick={() => navigate(`/add-transaction?to=${account.address}`)}>
                            Send coins to this address
                        </button>
                    </div>
                </div>
            )}

            {account && history && (
                <div className="account-history">
                    <h3>Transactions</h3>
                    {history.length === 0 ? (
                        <p className="small">None in the main chain yet.</p>
                    ) : (
                        <table>
                            <thead>
                                <tr><th></th><th>Coins</th><th>With</th><th>Block</th><th>Confirmations</th><th>Transaction</th></tr>
                            </thead>
                            <tbody>
                                {history.map((entry) => {
                                    const tx = entry.transaction;
                                    const other = entry.direction === 'received' ? tx.sender : tx.receiver;
                                    return (
                                        <tr key={toHex(tx.hash)} className={`direction-${entry.direction}`}>
                                            <td>{entry.direction === 'received' ? 'in' : entry.direction === 'sent' ? 'out' : 'self'}</td>
                                            <td>{entry.direction === 'sent' ? '−' : entry.direction === 'received' ? '+' : ''}{tx.amount}</td>
                                            <td><Hash value={toHex(other)} short to={accountLink(other)} /></td>
                                            <td><Hash value={toHex(entry.block)} short to={blockLink(entry.block)} /> <span className="small">#{entry.depth}</span></td>
                                            <td>{entry.confirmations}</td>
                                            <td><Hash value={toHex(tx.hash)} short /></td>
                                        </tr>
                                    );
                                })}
                            </tbody>
                        </table>
                    )}
                    {history.length === 100 && <p className="small">Showing the latest 100.</p>}
                </div>
            )}
            {!address && !error && (
                <p className="small">Paste an address, or follow one from a block, transaction or the wallet.</p>
            )}
        </div>
    );
};

export default Account;
