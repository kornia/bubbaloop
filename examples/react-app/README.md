# Bubbaloop React App

This is a React application for connecting to and displaying camera streams and inference results from a Bubbaloop backend.

https://github.com/user-attachments/assets/2ba6575a-6a47-43d7-a8eb-0cdb76be1d43

## Prerequisites

Before running the React app, you must have the Bubbaloop server running with the inference pipeline:

1. **Start the Bubbaloop server**
   ```bash
   # In the root of your bubbaloop project
   
   # First, start the server
   just serve 0.0.0.0 3000
   
   # Then, in another terminal, start the inference pipeline
   just start-pipeline inference 0.0.0.0 3000
   ```

2. **Verify the server is running**
   The server should start the inference pipeline and be ready to accept connections.
   You should see logs indicating successful initialization.

## Running the Application

Once the server is running, follow these steps to run the React app:

1. **Install dependencies**
   ```bash
   cd examples/react-app
   npm install
   ```

2. **Start the development server**
   ```bash
   npm run dev
   ```

3. **Access the application**
   The app will be available at http://localhost:5173 (or another port if 5173 is in use)

## Using the Application

1. **Configure connection settings**
   - The app includes a connection settings panel to configure the host and port of your backend services
   - Default values are set to 0.0.0.0:3000
   - Update these values to match your backend server configuration
   - Click "Update Connection" to apply changes

2. **Features**
<<<<<<< HEAD
=======



>>>>>>> main
   - Stream viewer for camera feeds
   - Inference instruction panel for sending commands
   - Inference result display showing detection outcomes

## Requirements

- Node.js (v16 or later recommended)
- A running Bubbaloop backend service
