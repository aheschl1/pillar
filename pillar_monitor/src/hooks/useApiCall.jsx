import { useState } from 'react';
import { useServer } from '../contexts/serverContext';

export const useApiCall = () => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);

    const call = async (endpoint) => {
        if (!isConnected) {
            throw new Error('Not connected to server');
        }

        setLoading(true);
        setError(null);

        try {
            const response = await fetch(`http://${ipAddress}:${httpPort}${endpoint}`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const result = await response.json();
            if (result.success && result.body !== undefined) {
                return result.body;
            } else {
                throw new Error(result.error || `Failed to fetch from ${endpoint}`);
            }
        } catch (e) {
            setError(e.message);
            throw e;
        } finally {
            setLoading(false);
        }
    };

    return { call, loading, error };
};
