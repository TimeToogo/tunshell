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
  margin-bottom: 30px;

  > *:first-child {
    margin-right: 15px;
  }

  button {
    font-size: 18px;
  }

  a {
    text-decoration: none;
  }
`;

export default function Home() {
  return (
    <div className="container">
      <Head>
        <title>Tunshell - Remote shell into empheral environments</title>
        <meta
          name="description"
          content="Tunshell is a simple and secure method to remote shell into ephemeral environments such as deployment pipelines or serverless functions."
        />
      </Head>

      <Header />
      <Hero />

      <CTA>
        <Link href="/go">
          <Button mode="inverted">Get started</Button>
        </Link>
        <a href="https://github.com/TimeToogo/tunshell#readme">
          <Button>README.md</Button>
        </a>
      </CTA>
    </div>
  );
}
