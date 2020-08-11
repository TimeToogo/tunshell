import React from "react";
import Head from "next/head";
import { Header } from "../components/header";
import { Wizard } from "../components/wizard";
import { Donate } from "../components/donate";

export default function Go() {
  return (
    <div className="container">
      <Head>
        <title>Tunshell</title>
      </Head>

      <Header />
      <Wizard />
      <Donate />
    </div>
  );
}
