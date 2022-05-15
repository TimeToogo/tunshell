import React, { useEffect, useState } from "react";
import Head from "next/head";
import { Header } from "../components/header";
import { DirectWebSession } from "../components/direct-web-term";

export default function Web() {
  return (
    <div className="container">
      <Head>
        <title>Direct Web Session - Tunshell</title>
      </Head>

      <Header />
      <DirectWebSession />
    </div>
  );
}
