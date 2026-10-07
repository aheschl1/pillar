import React from 'react';
import Hash from './Hash';
import { accountLink } from '../api/utils';
import './TransactionView.css';

const TransactionView = ({ txs }) => {
    if (!txs || txs.length === 0) {
        return <div className="small">No transactions</div>;
    }

    return (
        <div className="tx-grid">
            {txs.map((tx, idx) => (
                <div key={idx} className="tx-card">
                    {tx.error ? (
                        <div className="small error">Failed to load transaction {tx.hash}: {tx.error}</div>
                    ) : (
                        <>
                            <div className="tx-transfer">
                                <span className="tx-amount">{tx.amount}</span>
                                <span className="small">nonce {tx.nonce}</span>
                            </div>
                            <div className="tx-row"><span className="field-label">From</span><Hash value={tx.sender} to={accountLink(tx.sender)} /></div>
                            <div className="tx-row"><span className="field-label">To</span><Hash value={tx.receiver} to={accountLink(tx.receiver)} /></div>
                            <div className="tx-row"><span className="field-label">Hash</span><Hash value={tx.hash} /></div>
                            <div className="tx-row"><span className="field-label">Signature</span><Hash value={tx.signature} short /></div>
                        </>
                    )}
                </div>
            ))}
        </div>
    );
};

export default TransactionView;
