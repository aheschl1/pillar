import React from 'react';
import { toHex } from '../api/utils';
import './StateView.css';

const StateView = ({ accounts }) => {
    if (!accounts || accounts.length === 0) {
        return <div className="small">No account states for this block.</div>;
    }

    return (
        <div className="state-view">
            <h4>Account States ({accounts.length})</h4>
            <div className="state-table-container">
                <table className="state-table">
                    <thead>
                        <tr>
                            <th>Address</th>
                            <th>Balance</th>
                            <th>Nonce</th>
                            <th>Reputation</th>
                        </tr>
                    </thead>
                    <tbody>
                        {accounts.map((account, idx) => (
                            <tr key={idx}>
                                <td className="monospace" title={toHex(account.address)}>{toHex(account.address)}</td>
                                <td>{account.balance}</td>
                                <td>{account.nonce}</td>
                                <td>{account.reputation.toFixed(4)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
};

export default StateView;
