import React from 'react';
import './globals.css';
import ClientLayout from './ClientLayout';

export const metadata = {
  title: 'MoatAgeex Control Plane',
  description: 'Enterprise Administrative Dashboard for MoatAgeex Operating Layer',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body>
        <ClientLayout>{children}</ClientLayout>
      </body>
    </html>
  );
}
