// InferenceInstruction.tsx
import React, { useState } from 'react';
import './InferenceInstruction.css';

interface InferenceInstructionProps {
    settingsUrl: string;
    placeholder?: string;
    buttonText?: string;
    onSettingsSubmitted?: (success: boolean, response?: any) => void;
}

const InferenceInstruction: React.FC<InferenceInstructionProps> = ({
    settingsUrl,
    placeholder = 'cap en',
    buttonText = 'Apply',
    onSettingsSubmitted
}) => {
    const [prompt, setPrompt] = useState<string>('');
    const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
    const [error, setError] = useState<string | null>(null);

    const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
        setPrompt(e.target.value);
        if (error) setError(null);
    };

    const handleSubmit = async () => {
        if (!prompt.trim()) {
            setError('Please enter prompt text');
            return;
        }

        setIsSubmitting(true);
        setError(null);

        try {
            const response = await fetch(settingsUrl, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ prompt: prompt.trim() + '\n' }),
            });

            if (!response.ok) {
                throw new Error(`HTTP error! Status: ${response.status}`);
            }

            const result = await response.json();
            if (onSettingsSubmitted) {
                onSettingsSubmitted(true, result);
            }

            // Optional: clear the input after successful submission
            // setInstruction('');
        } catch (err) {
            const message = err instanceof Error ? err.message : 'Failed to submit instructions';
            setError(message);
            console.error('Instruction submission error:', message);
            if (onSettingsSubmitted) {
                onSettingsSubmitted(false);
            }
        } finally {
            setIsSubmitting(false);
        }
    };

    return (
        <div className="inference-instruction">
            <textarea
                className="instruction-input"
                value={prompt}
                onChange={handleInputChange}
                placeholder={placeholder}
                rows={3}
                disabled={isSubmitting}
            />

            <div className="instruction-controls">
                {error && <div className="instruction-error">{error}</div>}
                <button
                    className="instruction-button"
                    onClick={handleSubmit}
                    disabled={isSubmitting}
                >
                    {isSubmitting ? 'Sending...' : buttonText}
                </button>
            </div>
        </div>
    );
};

export default InferenceInstruction;