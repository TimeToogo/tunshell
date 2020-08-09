import React, { useState, useEffect } from "react";
import Head from "next/head";
import { Header } from "../components/header";

export default function Home() {
  return (
    <div className="container">
      <Head>
        <title>Tunshell - Zero-setup remote terminals</title>
      </Head>

      <Header />
    </div>
  );
}
