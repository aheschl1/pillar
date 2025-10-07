import { useState, useCallback } from 'react';
import { useHttp } from './useHttp';

export const useAddPeer = () => {
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);
    const [success, setSuccess] = useState(false);
    const { post } = useHttp(); // We will add post to useHttp

    const addPeer = useCallback(async (peerData) => {
        setLoading(true);
        setError(null);
        setSuccess(false);

        try {
            let url = '/peer';
            if (peerData.public_key) {
                url = `/peer/${peerData.public_key}`;
            }
            const response = await post(url, { ip_address: peerData.ip_address, port: parseInt(peerData.port, 10) });
            if (response.success) {
                setSuccess(true);
                return true;
            } else {
                setError(response.error || 'Failed to add peer');
                return false;
            }
        } catch (e) {
            setError(e.message);
            return false;
        } finally {
            setLoading(false);
        }
    }, [post]);

    return { addPeer, loading, error, success };
};
