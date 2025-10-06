import React from 'react';
import { BrowserRouter as Router, Routes, Route, NavLink } from 'react-router-dom';
import Overview from '../../pages/Overview';
import Peers from '../../pages/Peers';
import Chain from '../../pages/Chain';
import './Dashboard.css';

const Dashboard = () => {
    return (
        <Router>
            <div className="dashboard-container">
                <nav className="dashboard-nav">
                    <ul>
                        <li><NavLink to="/" className={({ isActive }) => isActive ? "active-link" : ""}>Overview</NavLink></li>
                        <li><NavLink to="/peers" className={({ isActive }) => isActive ? "active-link" : ""}>Peers</NavLink></li>
                        <li><NavLink to="/chain" className={({ isActive }) => isActive ? "active-link" : ""}>Chain</NavLink></li>
                    </ul>
                </nav>
                <div className="dashboard-content">
                    <Routes>
                        <Route path="/" element={<Overview />} />
                        <Route path="/peers" element={<Peers />} />
                        <Route path="/chain" element={<Chain />} />
                    </Routes>
                </div>
            </div>
        </Router>
    );
};

export default Dashboard;
