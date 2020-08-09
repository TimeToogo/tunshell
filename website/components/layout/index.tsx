import React from "react";
import styled from "styled-components";

export const Container = styled.div`
  width: 1200px;
  max-width: 100%;
  height: 100%;
  margin: 0 auto;

  @media (max-width: 1200px) {
    width: 1000px;
  }

  @media (max-width: 1000px) {
    padding: 0 15px;
  }
`;
