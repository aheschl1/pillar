import React, { useState } from 'react';
import { useHttp } from '../hooks/useHttp';
import Hash from '../components/Hash';
import { accountLink } from '../api/utils';
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
                    <button onClick={refetch} className="secondary">Refresh</button>
                </div>
            </div>

            <p className="small">The wallet this node signs with. Its address is where coins sent to this node arrive.</p>
            {loading && <p className="small">Loading…</p>}
            {error && <div className="error-box">{error}</div>}

            {data && (
                <div className="wallet-card">
                    <div className="card-top">
                        <div className="card-balance">{data.balance} <span>coins</span></div>
                    </div>
                    <div className="card-body">
                        <div className="card-row">
                            <div className="label">Address</div>
                            <div className="value"><Hash value={data.public_key} to={accountLink(data.public_key)} /></div>
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
