import React from 'react';
import Hash from './Hash';
import { accountLink } from '../api/utils';
import './StateView.css';

const StateView = ({ accounts }) => {
    if (!accounts || accounts.length === 0) {
        return <div className="small">No account states for this block.</div>;
    }

    return (
        <div className="state-view">
            <h4>Accounts after this block ({accounts.length})</h4>
            <div className="state-table-container">
                <table className="state-table">
                    <thead>
                        <tr>
                            <th>Address</th>
                            <th className="num">Balance</th>
                            <th className="num">Nonce</th>
                            <th className="num">Reputation</th>
                        </tr>
                    </thead>
                    <tbody>
                        {accounts.map((account, idx) => (
                            <tr key={idx}>
                                <td><Hash value={account.address} short to={accountLink(account.address)} /></td>
                                <td className="num">{account.balance}</td>
                                <td className="num">{account.nonce}</td>
                                <td className="num">{account.reputation.toFixed(2)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
};

export default StateView;
