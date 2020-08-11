import styled from "styled-components";
import { Container } from "../layout";

export const Donate = styled.div`
  width: 100%;
  margin: 40px 0;

  ${Container} {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  h4 {
    font-weight: normal;
  }
`;

export const Button = styled.div`
  margin: 0 auto;
`;
