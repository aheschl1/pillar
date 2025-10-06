import { useState, useEffect } from 'react';
import { useServer } from '../contexts/serverContext';

export const useHttp = (endpoint) => {
    const { ipAddress, httpPort, isConnected } = useServer();
    const [data, setData] = useState(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);

    useEffect(() => {
        const fetchData = async () => {
            if (!isConnected || !endpoint) {
                setData(null);
                return;
            }

            setLoading(true);
            setError(null);

            try {
                const response = await fetch(`http://${ipAddress}:${httpPort}${endpoint}`);
                if (!response.ok) {
                    throw new Error(`HTTP error! status: ${response.status}`);
                }
                const result = await response.json();
                if (result.success && result.body) {
                    setData(result.body);
                } else {
                    throw new Error(result.error || `Failed to fetch from ${endpoint}`);
                }
            } catch (e) {
                setError(e.message);
                setData(null);
            } finally {
                setLoading(false);
            }
        };

        fetchData();

        const intervalId = setInterval(fetchData, 5000); // Poll every 5 seconds

        return () => clearInterval(intervalId);

    }, [ipAddress, httpPort, isConnected, endpoint]);

    return { data, loading, error };
};
