import React, { useState, useEffect } from "react";
import Head from "next/head";
import { Header } from "../components/header";
import { Hero } from "../components/hero";
import styled from "styled-components";
import { Link } from "../components/link";
import { Button } from "../components/button";

const CTA = styled.div`
  width: 100%;
  display: flex;
  justify-content: center;
  button {
    font-size: 18px;
  }
`;

export default function Home() {
  return (
    <div className="container">
      <Head>
        <title>Tunshell - Zero-setup remote terminals</title>
      </Head>

      <Header />
      <Hero />

      <CTA>
        <Link href="/go">
          <Button mode="inverted">Get started</Button>
        </Link>
      </CTA>
    </div>
  );
}
