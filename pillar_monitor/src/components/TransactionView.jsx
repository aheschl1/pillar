import React, { useState } from 'react';
import { toHex } from '../api/utils';
import './TransactionView.css';

function SignatureField({ signature }) {
    const copyToClipboard = async () => {
        try {
            await navigator.clipboard.writeText(typeof signature === 'string' ? signature : JSON.stringify(signature));
            // Optional: brief visual feedback could be added here
        } catch (e) {
            console.error('Clipboard write failed', e);
        }
    };

    const display = typeof signature === 'string' ? signature : JSON.stringify(signature);
    const preview = display.length > 40 ? display.slice(0, 40) + '…' : display;

    return (
        <div className="tx-row signature-row">
            <strong>Signature:</strong>
            <div className="signature-controls">
                <code className="monospace signature-text" title={display}>{preview}</code>
                <div className="sig-buttons">
                    <button type="button" className="copy-btn" onClick={copyToClipboard}>Copy</button>
                </div>
            </div>
        </div>
    );
}

const TransactionView = ({ txs }) => {
    if (!txs || txs.length === 0) {
        return <div className="small">No transactions</div>;
    }

    return (
        <div className="tx-grid">
            {txs.map((tx, idx) => (
                <div key={idx} className="tx-card">
                    {tx.error ? (
                        <div className="small error">Failed to load tx {tx.hash}: {tx.error}</div>
                    ) : (
                        <div className="tx-content">
                            <div className="tx-row"><strong>Hash:</strong> <span className="monospace">{toHex(tx.hash)}</span></div>
                            <div className="tx-row"><strong>Sender:</strong> <span className="monospace">{toHex(tx.sender)}</span></div>
                            <div className="tx-row"><strong>Receiver:</strong> <span className="monospace">{toHex(tx.receiver)}</span></div>
                            <div className="tx-row"><strong>Amount:</strong> {tx.amount}</div>
                            <div className="tx-row"><strong>Nonce:</strong> {tx.nonce}</div>
                            <SignatureField signature={tx.signature} />
                        </div>
                    )}
                </div>
            ))}
        </div>
    );
};

export default TransactionView;
