import React from 'react';
import { useParams, Link } from 'react-router-dom';
import { useHttp } from '../hooks/useHttp';
import { toHex } from '../api/utils';
import './Transaction.css';

const Transaction = () => {
    const { hash } = useParams();
    const { data, loading, error } = useHttp(`/transaction/${hash}`, true);

    return (
        <div className="transaction-container">
            <div className="transaction-header">
                <h2>Transaction Details</h2>
                <Link to="/chain" className="back-link">← Back to Chain</Link>
            </div>

            {loading && <p>Loading transaction...</p>}
            {error && <p className="error-message">{error}</p>}

            {data && (
                <div className="transaction-card">
                    <div className="transaction-section">
                        <h3>Overview</h3>
                        <div className="transaction-field">
                            <strong>Hash:</strong>
                            <span className="monospace">{toHex(data.hash)}</span>
                        </div>
                        <div className="transaction-field">
                            <strong>Timestamp:</strong>
                            <span>{new Date(data.header.timestamp).toLocaleString()}</span>
                        </div>
                    </div>

                    <div className="transaction-section">
                        <h3>Transfer Details</h3>
                        <div className="transaction-field">
                            <strong>From (Sender):</strong>
                            <span className="monospace">{toHex(data.header.sender)}</span>
                        </div>
                        <div className="transaction-field">
                            <strong>To (Receiver):</strong>
                            <span className="monospace">{toHex(data.header.receiver)}</span>
                        </div>
                        <div className="transaction-field">
                            <strong>Amount:</strong>
                            <span className="amount">{data.header.amount} native currency</span>
                        </div>
                    </div>

                    <div className="transaction-section">
                        <h3>Technical Details</h3>
                        <div className="transaction-field">
                            <strong>Nonce:</strong>
                            <span>{data.header.nonce}</span>
                        </div>
                        <div className="transaction-field">
                            <strong>Signature:</strong>
                            <span className="monospace signature">
                                {data.signature.map(b => b.toString(16).padStart(2, '0')).join('')}
                            </span>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
};

export default Transaction;
