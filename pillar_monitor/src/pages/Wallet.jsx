import React, { useState } from 'react';
import { useHttp } from '../hooks/useHttp';
import './Wallet.css';

const Wallet = () => {
    const { data, loading, error, refetch } = useHttp('/wallet', false);
    const [showPrivate, setShowPrivate] = useState(false);

    const [toast, setToast] = useState('');
    const copyToClipboard = async (text) => {
        try {
            await navigator.clipboard.writeText(text);
            setToast('Copied to clipboard');
            setTimeout(() => setToast(''), 2000);
        } catch (e) {
            console.error('copy failed', e);
            setToast('Copy failed');
            setTimeout(() => setToast(''), 2000);
        }
    };

    return (
        <div className="wallet-container">
            <div className="wallet-header">
                <h2>Wallet</h2>
                <div>
                    <button onClick={refetch} className="refresh">Refresh</button>
                </div>
            </div>

            {loading && <p>Loading wallet...</p>}
            {error && <p className="error-message">{error}</p>}

            {data && (
                <div className="wallet-card">
                    <div className="card-top">
                        <div className="card-balance">{data.balance} native currency</div>
                    </div>
                    <div className="card-body">
                        <div className="card-row">
                            <div className="label">Public Key</div>
                            <div className="value"><code>{Array.isArray(data.public_key) ? data.public_key.map(b => b.toString(16).padStart(2, '0')).join('') : data.public_key}</code>
                                <button className="copy" onClick={() => copyToClipboard(Array.isArray(data.public_key) ? data.public_key.join(',') : data.public_key)}>Copy</button>
                            </div>
                        </div>
                        <div className="card-row">
                            <div className="label">Nonce</div>
                            <div className="value">{data.nonce}</div>
                        </div>
                        <div className="card-row private-row">
                            <div className="label">Private Key</div>
                            <div className="value private">
                                <code className={showPrivate ? 'reveal' : 'blur'}>{data.private_key}</code>
                                <div className="private-actions">
                                    <button className="toggle" onClick={() => setShowPrivate(s => !s)}>{showPrivate ? 'Hide' : 'Show'}</button>
                                    <button className="copy" onClick={() => copyToClipboard(data.private_key)}>Copy</button>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            )}
            {toast && <div className="wallet-toast">{toast}</div>}
        </div>
    );
};

export default Wallet;
